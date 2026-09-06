mod budget;
mod cache;
mod focus;
mod formatter;
mod parser;
mod walker;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use sha2::{Digest, Sha256};

use budget::{
    cap_file_tokens, count_text_tokens, downgrade_largest_file, effective_priority_score,
    optimize_budget, ProcessedFile, TokenCounter, TokenizerKind,
};
use cache::{cache_path_for_root, CacheDiagnostics, CacheMetadata, CacheStatus, ParseCache};
use focus::apply_request_focus;
use formatter::{
    format_repository_context_json, format_repository_context_text, format_repository_context_xml,
    DirectorySummary, FormatOptions, ProjectMapMode as FormatProjectMapMode, RepositoryMetadata,
};
use parser::{compress_file, parser_support_for_extension, CompressionLevel, ParserMode};
use walker::{
    collect_code_files, is_supported_path, matches_path_filters, supported_extensions,
    WalkerOptions,
};

const DEFAULT_MAX_TOKENS: usize = 12000;
const DEFAULT_MAX_FILE_BYTES: u64 = 1_048_576;
const DEFAULT_LEVEL: u8 = 1;
const DEFAULT_OUTPUT_FILE: &str = "bonsai.xml";

#[derive(Debug, Parser)]
#[command(name = "bonsai")]
#[command(version)]
#[command(about = "Shrink repository source context into token-efficient XML, JSON, or text")]
struct Cli {
    #[arg(default_value = ".", help_heading = "Basic")]
    path: PathBuf,

    #[arg(
        long,
        value_name = "PATH",
        help = "Read settings from this file instead of the repository's .bonsai.toml",
        help_heading = "Basic"
    )]
    config: Option<PathBuf>,

    #[arg(
        long,
        value_enum,
        help = "Simple flow: full (default), changed (local cache), map (project map), prompt (clipboard)",
        help_heading = "Basic"
    )]
    preset: Option<Preset>,

    #[arg(long, default_value_t = DEFAULT_MAX_TOKENS, help_heading = "Budget")]
    max_tokens: usize,

    #[arg(
        long,
        default_value_t = TokenizerKind::default(),
        value_name = "TOKENIZER",
        help = "Tokenizer family or model alias: o200k_base, cl100k_base, p50k_base, p50k_edit, r50k_base",
        help_heading = "Budget"
    )]
    tokenizer: TokenizerKind,

    #[arg(
        long,
        default_value_t = DEFAULT_MAX_FILE_BYTES,
        help = "Skip files larger than this many bytes; 0 disables the cap",
        help_heading = "Budget"
    )]
    max_file_bytes: u64,

    #[arg(
        long,
        help = "Cap each file to this many tokens before global budget optimization; 0 disables the cap",
        help_heading = "Budget"
    )]
    max_file_tokens: Option<usize>,

    #[arg(
        long,
        default_value_t = DEFAULT_LEVEL,
        help = "Compression: 1 preserves full source and fails if it cannot fit (default), 2 allows lossy signatures and shapes, 3 allows a lossy tree map",
        help_heading = "Budget"
    )]
    level: u8,

    #[arg(long, value_enum, default_value_t = OutputDestination::File, help_heading = "Output")]
    output: OutputDestination,

    #[arg(long, default_value = DEFAULT_OUTPUT_FILE, help_heading = "Output")]
    output_file: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Xml, help_heading = "Output")]
    format: OutputFormat,

    #[arg(long, help = "Write only the project map", help_heading = "Output")]
    project_map_only: bool,

    #[arg(
        long,
        value_name = "TOKENS",
        help = "When --max-tokens is below this value, output metadata, project map, and directory summaries only",
        help_heading = "Output"
    )]
    map_only_under: Option<usize>,

    #[arg(long, value_enum, default_value_t = ProjectMapMode::Flat, help_heading = "Output")]
    project_map: ProjectMapMode,

    #[arg(
        long,
        help = "Include stable content hashes in project map entries",
        help_heading = "Output"
    )]
    file_hashes: bool,

    #[arg(
        long,
        help = "Omit token count fields from XML and JSON output",
        help_heading = "Output"
    )]
    no_token_counts: bool,

    #[arg(
        long,
        help = "Omit file bodies while keeping metadata and the project map",
        help_heading = "Output"
    )]
    no_content: bool,

    #[arg(
        long,
        help = "Print selected files and estimated tokens without writing output",
        help_heading = "Output"
    )]
    dry_run: bool,

    #[arg(long, value_enum, default_value_t = SortMode::Path, help_heading = "Output")]
    sort: SortMode,

    #[arg(
        long,
        help = "Add token totals for each directory",
        help_heading = "Output"
    )]
    directory_summaries: bool,

    #[arg(
        long,
        help = "Fail if the final output is still over budget",
        help_heading = "Output"
    )]
    fail_over_budget: bool,

    #[arg(
        long,
        help = "Omit lowest-priority files if tree-map output still exceeds --max-tokens",
        help_heading = "Output"
    )]
    drop_low_priority: bool,

    #[arg(
        long,
        help = "Changed workflow: after one normal run, include only added or changed files from the local cache",
        help_heading = "Changes"
    )]
    incremental: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Only include files added or changed compared with a base directory or cache file",
        help_heading = "Changes"
    )]
    incremental_base: Option<PathBuf>,

    #[arg(
        long,
        value_name = "GIT_REF",
        help = "Changed workflow: include tracked changes, untracked files, and deletions compared with this Git ref; do not combine with --incremental",
        help_heading = "Changes"
    )]
    changed_since: Option<String>,

    #[arg(
        long,
        help = "Print added, changed, unchanged, skipped, and deleted counts",
        help_heading = "Changes"
    )]
    incremental_summary: bool,

    #[arg(
        long,
        value_name = "GLOB",
        help = "Only include matching paths",
        help_heading = "Selection"
    )]
    include: Vec<String>,

    #[arg(
        long,
        value_name = "GLOB",
        help = "Exclude matching paths",
        help_heading = "Selection"
    )]
    exclude: Vec<String>,

    #[arg(
        long = "no-respect-gitignore",
        action = ArgAction::SetFalse,
        default_value_t = true,
        help = "Include files ignored by Git",
        help_heading = "Selection"
    )]
    respect_gitignore: bool,

    #[arg(
        long,
        help = "Skip minified, vendored, generated, and lockfile-like files unless --include matches them",
        help_heading = "Selection"
    )]
    exclude_generated: bool,

    #[arg(
        long,
        value_name = "TEXT",
        help = "Request text: keep modules matching the request and their dependencies at higher detail while shrinking background files. Applies lossy shrinking only at --level 2 or 3; level 1 keeps full source",
        help_heading = "Selection"
    )]
    focus: Option<String>,

    #[arg(
        long,
        help = "Print selected files before generation",
        help_heading = "Diagnostics"
    )]
    print_files: bool,

    #[arg(
        long,
        help = "Fail when no supported files are selected",
        help_heading = "Diagnostics"
    )]
    fail_on_empty: bool,

    #[arg(
        long,
        help = "Suppress normal stdout output for scripts",
        help_heading = "Diagnostics"
    )]
    quiet: bool,

    #[arg(
        long,
        help = "Print a summary and output token count",
        help_heading = "Diagnostics"
    )]
    stats: bool,

    #[arg(
        long,
        help = "Print per-file and token distribution details",
        help_heading = "Diagnostics"
    )]
    detailed_stats: bool,

    #[arg(
        long,
        help = "Print output path and selected file count",
        help_heading = "Diagnostics"
    )]
    summary: bool,

    #[arg(
        long,
        help = "Wrap output in a paste-ready agent prompt",
        help_heading = "Prompt"
    )]
    prompt: bool,

    #[arg(
        long,
        value_name = "TEXT",
        help = "Use this task text inside the prompt wrapper",
        help_heading = "Prompt"
    )]
    ask_template: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Check Bonsai and prepare the first-run output path")]
    Setup {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(
            long,
            default_value_t = TokenizerKind::default(),
            value_name = "TOKENIZER"
        )]
        tokenizer: TokenizerKind,

        #[arg(long, default_value = "bonsai.xml")]
        output_file: PathBuf,
    },

    #[command(about = "Write selectable AGENTS.md and CLAUDE.md starter instructions")]
    InitAgent {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long, short)]
        force: bool,

        #[arg(
            long,
            value_enum,
            default_value_t = AgentFileSelection::Both,
            value_name = "FILES",
            help = "Create agents, claude, or both files"
        )]
        files: AgentFileSelection,

        #[arg(
            long,
            value_enum,
            default_value_t = AgentInstructionStyle::Short,
            value_name = "STYLE",
            help = "Use short or detailed starter instructions"
        )]
        style: AgentInstructionStyle,

        #[arg(
            long,
            value_name = "TOKENS",
            help = "Add a token budget to the generated Bonsai command"
        )]
        max_tokens: Option<usize>,

        #[arg(
            long,
            value_name = "PATH",
            help = "Add a custom output file to the generated Bonsai command"
        )]
        output_file: Option<PathBuf>,
    },

    #[command(about = "Manage Bonsai cache")]
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },

    #[command(about = "Show install health")]
    Doctor {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(
            long,
            default_value_t = TokenizerKind::default(),
            value_name = "TOKENIZER"
        )]
        tokenizer: TokenizerKind,

        #[arg(long)]
        json: bool,
    },

    #[command(about = "Generate shell completions")]
    Completions { shell: CompletionShell },

    #[command(about = "Print the generated Markdown CLI option reference")]
    Docs,
}

#[derive(Debug, Subcommand)]
enum CacheCommands {
    #[command(about = "Clear the local parse cache for a repo")]
    Clear {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AgentFileSelection {
    Agents,
    Claude,
    Both,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AgentInstructionStyle {
    Short,
    Detailed,
}

impl CompletionShell {
    fn as_clap_shell(self) -> Shell {
        match self {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::Zsh => Shell::Zsh,
            CompletionShell::Fish => Shell::Fish,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputDestination {
    Clipboard,
    File,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
    Xml,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SortMode {
    Path,
    Tokens,
    Priority,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ProjectMapMode {
    Flat,
    Compact,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Preset {
    Full,
    Changed,
    Map,
    Prompt,
}

#[derive(Debug, Default)]
struct BonsaiConfigFile {
    preset: Option<Preset>,
    max_tokens: Option<usize>,
    tokenizer: Option<TokenizerKind>,
    max_file_bytes: Option<u64>,
    max_file_tokens: Option<usize>,
    level: Option<u8>,
    output: Option<OutputDestination>,
    output_file: Option<PathBuf>,
    format: Option<OutputFormat>,
    project_map_only: Option<bool>,
    incremental: Option<bool>,
    changed_since: Option<String>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    respect_gitignore: Option<bool>,
    exclude_generated: Option<bool>,
    focus: Option<String>,
}

fn apply_config(cli: &mut Cli, root: &Path, args: &[OsString]) -> Result<()> {
    let Some(path) = config_path(cli, root)? else {
        return Ok(());
    };
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("cannot read config file {}", path.display()))?;
    let config = parse_config(&contents, &path)?;

    if !option_was_provided(args, "--preset") {
        if let Some(value) = config.preset {
            cli.preset = Some(value);
        }
    }
    if !option_was_provided(args, "--max-tokens") {
        if let Some(value) = config.max_tokens {
            cli.max_tokens = value;
        }
    }
    if !option_was_provided(args, "--tokenizer") {
        if let Some(value) = config.tokenizer {
            cli.tokenizer = value;
        }
    }
    if !option_was_provided(args, "--max-file-bytes") {
        if let Some(value) = config.max_file_bytes {
            cli.max_file_bytes = value;
        }
    }
    if !option_was_provided(args, "--max-file-tokens") {
        if let Some(value) = config.max_file_tokens {
            cli.max_file_tokens = Some(value);
        }
    }
    if !option_was_provided(args, "--level") {
        if let Some(value) = config.level {
            cli.level = value;
        }
    }
    if !option_was_provided(args, "--output") {
        if let Some(value) = config.output {
            cli.output = value;
        }
    }
    if !option_was_provided(args, "--output-file") {
        if let Some(value) = config.output_file {
            cli.output_file = value;
        }
    }
    if !option_was_provided(args, "--format") {
        if let Some(value) = config.format {
            cli.format = value;
        }
    }
    if !option_was_provided(args, "--project-map-only") {
        if let Some(value) = config.project_map_only {
            cli.project_map_only = value;
        }
    }
    if !option_was_provided(args, "--incremental") {
        if let Some(value) = config.incremental {
            cli.incremental = value;
        }
    }
    if !option_was_provided(args, "--changed-since") {
        if let Some(value) = config.changed_since {
            cli.changed_since = Some(value);
        }
    }
    if !option_was_provided(args, "--include") {
        if let Some(value) = config.include {
            cli.include = value;
        }
    }
    if !option_was_provided(args, "--exclude") {
        if let Some(value) = config.exclude {
            cli.exclude = value;
        }
    }
    if !option_was_provided(args, "--no-respect-gitignore") {
        if let Some(value) = config.respect_gitignore {
            cli.respect_gitignore = value;
        }
    }
    if !option_was_provided(args, "--exclude-generated") {
        if let Some(value) = config.exclude_generated {
            cli.exclude_generated = value;
        }
    }
    if !option_was_provided(args, "--focus") {
        if let Some(value) = config.focus {
            cli.focus = Some(value);
        }
    }

    Ok(())
}

fn apply_preset(cli: &mut Cli, args: &[OsString]) {
    let Some(preset) = cli.preset else {
        return;
    };

    match preset {
        Preset::Full => {}
        Preset::Changed => {
            if cli.changed_since.is_none() && cli.incremental_base.is_none() {
                cli.incremental = true;
            }
            if !option_was_provided(args, "--incremental-summary") {
                cli.incremental_summary = true;
            }
        }
        Preset::Map => {
            if !option_was_provided(args, "--project-map-only") {
                cli.project_map_only = true;
            }
        }
        Preset::Prompt => {
            cli.prompt = true;
            if !option_was_provided(args, "--output") {
                cli.output = OutputDestination::Clipboard;
            }
        }
    }
}

fn config_path(cli: &Cli, root: &Path) -> Result<Option<PathBuf>> {
    if let Some(path) = &cli.config {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            env::current_dir()
                .context("cannot resolve current directory")?
                .join(path)
        };
        if !path.is_file() {
            bail!("config file does not exist: {}", path.display());
        }
        return Ok(Some(path));
    }

    let path = root.join(".bonsai.toml");
    if path.exists() && !path.is_file() {
        bail!("config path exists but is not a file: {}", path.display());
    }

    Ok(path.is_file().then_some(path))
}

fn option_was_provided(args: &[OsString], option: &str) -> bool {
    args.iter().skip(1).any(|argument| {
        let argument = argument.to_string_lossy();
        argument == option || argument.starts_with(&format!("{option}="))
    })
}

fn parse_config(contents: &str, path: &Path) -> Result<BonsaiConfigFile> {
    let mut config = BonsaiConfigFile::default();

    for (line_index, raw_line) in contents.lines().enumerate() {
        let line_number = line_index + 1;
        let line = strip_config_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            bail!(
                "invalid config {}:{}: expected key = value",
                path.display(),
                line_number
            );
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "preset" => config.preset = Some(parse_config_preset(value, path, line_number)?),
            "max_tokens" => {
                config.max_tokens = Some(parse_config_usize(value, key, path, line_number)?)
            }
            "tokenizer" => {
                config.tokenizer = Some(parse_config_tokenizer(value, path, line_number)?)
            }
            "max_file_bytes" => {
                config.max_file_bytes = Some(parse_config_u64(value, key, path, line_number)?)
            }
            "max_file_tokens" => {
                config.max_file_tokens = Some(parse_config_usize(value, key, path, line_number)?)
            }
            "level" => config.level = Some(parse_config_u8(value, key, path, line_number)?),
            "output" => config.output = Some(parse_config_output(value, path, line_number)?),
            "output_file" => {
                config.output_file = Some(PathBuf::from(parse_config_string(
                    value,
                    key,
                    path,
                    line_number,
                )?))
            }
            "format" => config.format = Some(parse_config_format(value, path, line_number)?),
            "project_map_only" => {
                config.project_map_only = Some(parse_config_bool(value, key, path, line_number)?)
            }
            "incremental" => {
                config.incremental = Some(parse_config_bool(value, key, path, line_number)?)
            }
            "changed_since" => {
                config.changed_since = Some(parse_config_string(value, key, path, line_number)?)
            }
            "include" => {
                config.include = Some(parse_config_strings(value, key, path, line_number)?)
            }
            "exclude" => {
                config.exclude = Some(parse_config_strings(value, key, path, line_number)?)
            }
            "respect_gitignore" => {
                config.respect_gitignore = Some(parse_config_bool(value, key, path, line_number)?)
            }
            "exclude_generated" => {
                config.exclude_generated = Some(parse_config_bool(value, key, path, line_number)?)
            }
            "focus" => config.focus = Some(parse_config_string(value, key, path, line_number)?),
            _ => bail!(
                "unknown config key `{key}` in {}:{}",
                path.display(),
                line_number
            ),
        }
    }

    Ok(config)
}

fn strip_config_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;

    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_quotes && character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..index],
            _ => {}
        }
    }

    line
}

fn parse_config_string(value: &str, key: &str, path: &Path, line_number: usize) -> Result<String> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        bail!(
            "invalid config {}:{}: {key} must be a quoted string",
            path.display(),
            line_number
        );
    }

    let mut parsed = String::new();
    let mut characters = value[1..value.len() - 1].chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            parsed.push(character);
            continue;
        }

        let Some(escaped) = characters.next() else {
            bail!(
                "invalid config {}:{}: unfinished escape in {key}",
                path.display(),
                line_number
            );
        };
        match escaped {
            '"' => parsed.push('"'),
            '\\' => parsed.push('\\'),
            'n' => parsed.push('\n'),
            'r' => parsed.push('\r'),
            't' => parsed.push('\t'),
            _ => bail!(
                "invalid config {}:{}: unsupported escape in {key}",
                path.display(),
                line_number
            ),
        }
    }

    Ok(parsed)
}

fn parse_config_strings(
    value: &str,
    key: &str,
    path: &Path,
    line_number: usize,
) -> Result<Vec<String>> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('[') || !value.ends_with(']') {
        bail!(
            "invalid config {}:{}: {key} must be an array of quoted strings",
            path.display(),
            line_number
        );
    }

    let values = value[1..value.len() - 1].trim();
    if values.is_empty() {
        return Ok(Vec::new());
    }

    values
        .split(',')
        .map(|item| parse_config_string(item.trim(), key, path, line_number))
        .collect()
}

fn parse_config_bool(value: &str, key: &str, path: &Path, line_number: usize) -> Result<bool> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => bail!(
            "invalid config {}:{}: {key} must be true or false",
            path.display(),
            line_number
        ),
    }
}

fn parse_config_usize(value: &str, key: &str, path: &Path, line_number: usize) -> Result<usize> {
    value.trim().parse().map_err(|_| {
        anyhow::anyhow!(
            "invalid config {}:{}: {key} must be a positive integer",
            path.display(),
            line_number
        )
    })
}

fn parse_config_u64(value: &str, key: &str, path: &Path, line_number: usize) -> Result<u64> {
    value.trim().parse().map_err(|_| {
        anyhow::anyhow!(
            "invalid config {}:{}: {key} must be a positive integer",
            path.display(),
            line_number
        )
    })
}

fn parse_config_u8(value: &str, key: &str, path: &Path, line_number: usize) -> Result<u8> {
    value.trim().parse().map_err(|_| {
        anyhow::anyhow!(
            "invalid config {}:{}: {key} must be a number",
            path.display(),
            line_number
        )
    })
}

fn parse_config_tokenizer(value: &str, path: &Path, line_number: usize) -> Result<TokenizerKind> {
    let value = parse_config_string(value, "tokenizer", path, line_number)?;
    value.parse().map_err(|error: String| {
        anyhow::anyhow!(
            "invalid config {}:{}: invalid tokenizer `{value}` ({error})",
            path.display(),
            line_number
        )
    })
}

fn parse_config_output(value: &str, path: &Path, line_number: usize) -> Result<OutputDestination> {
    let value = parse_config_string(value, "output", path, line_number)?.to_ascii_lowercase();
    match value.as_str() {
        "file" => Ok(OutputDestination::File),
        "clipboard" => Ok(OutputDestination::Clipboard),
        _ => bail!(
            "invalid config {}:{}: output must be file or clipboard",
            path.display(),
            line_number
        ),
    }
}

fn parse_config_format(value: &str, path: &Path, line_number: usize) -> Result<OutputFormat> {
    let value = parse_config_string(value, "format", path, line_number)?.to_ascii_lowercase();
    match value.as_str() {
        "xml" => Ok(OutputFormat::Xml),
        "json" => Ok(OutputFormat::Json),
        "text" => Ok(OutputFormat::Text),
        _ => bail!(
            "invalid config {}:{}: format must be xml, json, or text",
            path.display(),
            line_number
        ),
    }
}

fn parse_config_preset(value: &str, path: &Path, line_number: usize) -> Result<Preset> {
    let value = parse_config_string(value, "preset", path, line_number)?.to_ascii_lowercase();
    match value.as_str() {
        "full" => Ok(Preset::Full),
        "changed" => Ok(Preset::Changed),
        "map" => Ok(Preset::Map),
        "prompt" => Ok(Preset::Prompt),
        _ => bail!(
            "invalid config {}:{}: preset must be full, changed, map, or prompt",
            path.display(),
            line_number
        ),
    }
}

impl From<ProjectMapMode> for FormatProjectMapMode {
    fn from(mode: ProjectMapMode) -> Self {
        match mode {
            ProjectMapMode::Flat => Self::Flat,
            ProjectMapMode::Compact => Self::Compact,
        }
    }
}

fn main() -> Result<()> {
    let raw_args = env::args_os().collect::<Vec<_>>();
    let mut cli = Cli::parse_from(raw_args.clone());
    if let Some(command) = &cli.command {
        handle_command(command)?;
        return Ok(());
    }

    let root = fs::canonicalize(&cli.path)
        .with_context(|| format!("cannot resolve target path {}", cli.path.display()))?;
    apply_config(&mut cli, &root, &raw_args)?;
    apply_preset(&mut cli, &raw_args);
    validate_delta_options(&cli)?;
    let requested_level = CompressionLevel::try_from(cli.level)?;
    if requested_level == CompressionLevel::Full
        && (cli.drop_low_priority || cli.map_only_under.is_some())
    {
        bail!("level 1 preserves source: remove --drop-low-priority/--map-only-under, or explicitly choose --level 2 or 3 for lossy compression");
    }
    let token_counter = TokenCounter::new(cli.tokenizer)?;
    let mut parse_cache = ParseCache::load(cache_path_for_root(&root));
    let incremental_base = load_incremental_base(&cli)?;
    let git_changes = load_git_changes(&cli, &root)?;
    let cache_metadata = cache_metadata(&cli);
    let baseline_metadata_matches =
        baseline_metadata_matches(&incremental_base, &parse_cache, &cache_metadata);

    let paths = collect_code_files(
        &root,
        &WalkerOptions {
            include: cli.include.clone(),
            exclude: cli.exclude.clone(),
            respect_gitignore: cli.respect_gitignore,
            max_file_bytes: max_file_bytes(&cli),
            exclude_generated: cli.exclude_generated,
            output_file: output_exclusion(&cli),
        },
    )?;
    if paths.is_empty() {
        handle_empty_selection(&cli, &root)?;
    }

    if cli.print_files && !cli.quiet {
        print_selected_files(&root, &paths);
    }

    let current_relative_paths = relative_path_set(&root, &paths);
    let mut incremental_counts = IncrementalCounts::default();
    let mut files = Vec::with_capacity(paths.len());

    for path in paths {
        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let file_metadata = fs::metadata(&path)
            .with_context(|| format!("cannot read metadata for {}", path.display()))?;
        let delta = classify_file(
            &incremental_base,
            &git_changes,
            &parse_cache,
            baseline_metadata_matches,
            &path,
            &relative_path,
            &file_metadata,
        )?;
        let cached_variants = parse_cache.get(&path, &file_metadata);
        let include_file = should_include_delta(&cli, &incremental_base, delta);
        incremental_counts.record(delta, include_file);
        let variants = match cached_variants {
            Some(variants) => variants,
            None => {
                let variants = compress_file(&path, requested_level)
                    .with_context(|| format!("failed to parse {}", path.display()))?;
                parse_cache.put(&path, &file_metadata, variants.clone());
                variants
            }
        };

        if !include_file {
            continue;
        }

        let content_hash = if cli.file_hashes {
            variants.full.as_deref().map(stable_content_hash)
        } else {
            None
        };
        let mut file = ProcessedFile::new(relative_path, requested_level, variants);
        file.content_hash = content_hash;
        files.push(file);
    }
    let deleted_files = deleted_files(
        &cli,
        &incremental_base,
        &git_changes,
        &parse_cache,
        baseline_metadata_matches,
        &root,
        &current_relative_paths,
    )?;
    incremental_counts.deleted = deleted_files.len();
    parse_cache.retain_touched();
    parse_cache.set_metadata(cache_metadata);

    apply_request_focus(
        &mut files,
        requested_level,
        cli.focus.as_deref(),
        &token_counter,
    );

    if let Some(max_file_tokens) = max_file_tokens(&cli) {
        if requested_level == CompressionLevel::Full {
            for file in &files {
                let tokens = token_counter.count(file.content());
                if tokens > max_file_tokens {
                    bail!("{} needs {tokens} tokens, above --max-file-tokens {max_file_tokens}; level 1 preserves source. Increase the cap or explicitly choose --level 2 or 3 for lossy compression", file.path);
                }
            }
        } else {
            cap_file_tokens(&mut files, max_file_tokens, &token_counter);
        }
    }
    if uses_map_only_fallback(&cli) {
        set_files_to_tree_map(&mut files, &token_counter);
    }

    let raw_context = if cli.stats {
        let metadata = RepositoryMetadata {
            generated_at: generated_at_unix()?,
            repo_root: root.display().to_string(),
            max_tokens: cli.max_tokens,
            compression_level: requested_level.as_u8(),
            file_count: files.len(),
        };
        let mut full_files = full_context_files(&files, &token_counter)?;
        sort_files(&mut full_files, cli.sort);
        Some(maybe_wrap_prompt(
            format_context(&full_files, &metadata, &cli, &deleted_files, &[]),
            &cli,
        ))
    } else {
        None
    };

    let metadata = RepositoryMetadata {
        generated_at: generated_at_unix()?,
        repo_root: root.display().to_string(),
        max_tokens: cli.max_tokens,
        compression_level: requested_level.as_u8(),
        file_count: files.len(),
    };
    let content_budget =
        reserved_content_budget(&files, &metadata, &cli, &token_counter, &deleted_files)?;
    let optimized = if requested_level == CompressionLevel::Full {
        for file in &mut files {
            file.token_count = token_counter.count(file.content());
        }
        files
    } else {
        optimize_budget(files, content_budget, &token_counter)?
    };
    let (mut optimized, context, fitted_tokens, dropped_paths) =
        fit_formatted_context(optimized, &metadata, &cli, &token_counter, &deleted_files)?;
    sort_files(&mut optimized, cli.sort);
    // Verify the final emitted document: recount exactly what will be written
    // so the reported count, embedded warnings, and stderr notices agree.
    let output_tokens = count_text_tokens(&context, &token_counter);
    debug_assert_eq!(
        fitted_tokens, output_tokens,
        "fitted token count must match the final emitted document"
    );
    let warnings = final_warnings(&optimized, &dropped_paths, output_tokens, cli.max_tokens);
    if !warnings.is_empty() && !cli.quiet {
        for warning in &warnings {
            eprintln!("warning: {warning}");
        }
    }
    if output_tokens > cli.max_tokens && !cli.quiet {
        eprintln!(
            "warning: output is {output_tokens} tokens, above --max-tokens {}; treat this context as incomplete",
            cli.max_tokens
        );
    }
    let run_stats = RunStats::new(
        &cli,
        requested_level,
        optimized.len(),
        dropped_paths.len(),
        raw_context.as_deref(),
        output_tokens,
        &token_counter,
    )?;

    if output_tokens > cli.max_tokens {
        if cli.fail_over_budget {
            bail!(
                "output is {output_tokens} tokens, above --max-tokens {}",
                cli.max_tokens
            );
        }
    }

    if cli.dry_run {
        if !cli.quiet {
            print_dry_run(
                &optimized,
                &deleted_files,
                output_tokens,
                cli.max_tokens,
                &warnings,
            );
        }
    } else {
        match cli.output {
            OutputDestination::Clipboard => {
                let mut clipboard = arboard::Clipboard::new().context("cannot access clipboard")?;
                clipboard
                    .set_text(context)
                    .context("cannot write clipboard")?;
            }
            OutputDestination::File => {
                write_output_file(&cli.output_file, &context, cli.quiet)?;
            }
        }
    }

    if !cli.dry_run {
        if let Err(error) = parse_cache.save() {
            if !cli.quiet {
                eprintln!("warning: cannot write parse cache: {error:#}");
            }
        }
    }

    if !cli.dry_run && !cli.quiet {
        print_success(&run_stats);
    }

    if cli.summary && !cli.quiet {
        print_summary(&run_stats);
    }

    if cli.incremental_summary && !cli.quiet {
        print_incremental_summary(&incremental_counts);
    }

    if cli.stats && !cli.quiet {
        print_stats(&run_stats);
    }

    if cli.detailed_stats && !cli.quiet {
        print_detailed_stats(&optimized, &run_stats);
    }

    Ok(())
}

fn handle_command(command: &Commands) -> Result<()> {
    match command {
        Commands::Setup {
            path,
            tokenizer,
            output_file,
        } => run_setup(path, *tokenizer, output_file),
        Commands::InitAgent {
            path,
            force,
            files,
            style,
            max_tokens,
            output_file,
        } => init_agent_files(
            path,
            *force,
            *files,
            *style,
            *max_tokens,
            output_file.as_deref(),
        ),
        Commands::Cache { command } => match command {
            CacheCommands::Clear { path } => clear_cache(path),
        },
        Commands::Doctor {
            path,
            tokenizer,
            json,
        } => print_doctor(path, *tokenizer, *json),
        Commands::Completions { shell } => {
            let mut command = Cli::command();
            generate(
                shell.as_clap_shell(),
                &mut command,
                "bonsai",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        Commands::Docs => {
            print_cli_reference();
            Ok(())
        }
    }
}

fn run_setup(target: &Path, tokenizer: TokenizerKind, output_file: &Path) -> Result<()> {
    let root = fs::canonicalize(target)
        .with_context(|| format!("cannot resolve setup target {}", target.display()))?;
    let mut required_checks_ok = true;

    println!("bonsai setup:");
    println!("  version: {}", env!("CARGO_PKG_VERSION"));
    match env::current_exe() {
        Ok(binary_path) => match fs::metadata(&binary_path) {
            Ok(metadata) if metadata.is_file() => {
                println!("  binary: ok ({})", binary_path.display());
            }
            Ok(_) => {
                println!("  binary: failed ({}) is not a file", binary_path.display());
                required_checks_ok = false;
            }
            Err(error) => {
                println!("  binary: failed ({error})");
                required_checks_ok = false;
            }
        },
        Err(error) => {
            println!("  binary: failed ({error})");
            required_checks_ok = false;
        }
    }

    match TokenCounter::new(tokenizer) {
        Ok(_) => println!("  tokenizer: ok ({})", tokenizer.as_str()),
        Err(error) => {
            println!("  tokenizer: failed ({error:#})");
            required_checks_ok = false;
        }
    }

    let parser_reports = supported_extensions()
        .iter()
        .map(|extension| parser_support_for_extension(extension))
        .collect::<Vec<_>>();
    let unavailable_parsers = parser_reports
        .iter()
        .filter(|parser| !parser.available)
        .map(|parser| format!(".{}", parser.extension))
        .collect::<Vec<_>>();
    if unavailable_parsers.is_empty() {
        println!("  parsers: ok ({} available)", parser_reports.len());
    } else {
        println!(
            "  parsers: failed (unavailable: {})",
            unavailable_parsers.join(", ")
        );
        required_checks_ok = false;
    }

    match arboard::Clipboard::new() {
        Ok(_) => println!("  clipboard: ok"),
        Err(error) => println!("  clipboard: warning ({error:#}); file output still works"),
    }

    let output_exists = output_file.is_file();
    match prepare_output_path(output_file) {
        Ok(()) if output_exists => println!(
            "  output: ok ({} exists; the next write will warn before replacing it)",
            output_file.display()
        ),
        Ok(()) => println!("  output: ok ({})", output_file.display()),
        Err(error) => {
            println!("  output: failed ({error:#})");
            required_checks_ok = false;
        }
    }

    println!("  repository: {}", root.display());
    if required_checks_ok {
        println!("Ready. Run `bonsai .`.");
        Ok(())
    } else {
        bail!("setup found failed checks; fix them and run `bonsai setup` again")
    }
}

#[derive(Debug)]
struct DoctorReport {
    binary_path: PathBuf,
    version: &'static str,
    repo_root: PathBuf,
    cache_path: PathBuf,
    cache_size_bytes: u64,
    cache: CacheDiagnostics,
    tokenizer_name: String,
    tokenizer_status: String,
    tokenizer_ok: bool,
    parsers: Vec<DoctorParserReport>,
}

#[derive(Debug)]
struct DoctorParserReport {
    extension: String,
    mode: &'static str,
    available: bool,
}

fn print_doctor(target: &Path, tokenizer: TokenizerKind, json: bool) -> Result<()> {
    let report = doctor_report(target, tokenizer)?;

    if json {
        println!("{}", format_doctor_json(&report));
    } else {
        print_doctor_text(&report);
    }

    Ok(())
}

fn doctor_report(target: &Path, tokenizer: TokenizerKind) -> Result<DoctorReport> {
    let root = fs::canonicalize(target)
        .with_context(|| format!("cannot resolve doctor target {}", target.display()))?;
    let binary_path = env::current_exe().context("cannot resolve current executable")?;
    let cache_path = cache_path_for_root(&root);
    let tokenizer_result = TokenCounter::new(tokenizer).map(|_| ());
    let tokenizer_ok = tokenizer_result.is_ok();
    let tokenizer_status = tokenizer_result
        .map(|_| "ok".to_owned())
        .unwrap_or_else(|error| format!("error: {error:#}"));
    let cache_size_bytes = fs::metadata(&cache_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let cache = ParseCache::load(cache_path.clone()).diagnostics();
    let parsers = supported_extensions()
        .iter()
        .map(|extension| {
            let support = parser_support_for_extension(extension);
            let mode = match support.mode {
                ParserMode::TreeSitter => "tree-sitter",
                ParserMode::Compact => "compact",
            };
            DoctorParserReport {
                extension: format!(".{}", support.extension),
                mode,
                available: support.available,
            }
        })
        .collect();

    Ok(DoctorReport {
        binary_path,
        version: env!("CARGO_PKG_VERSION"),
        repo_root: root,
        cache_path,
        cache_size_bytes,
        cache,
        tokenizer_name: tokenizer.as_str().to_owned(),
        tokenizer_status,
        tokenizer_ok,
        parsers,
    })
}

fn print_doctor_text(report: &DoctorReport) {
    println!("bonsai doctor:");
    println!("  binary: {}", report.binary_path.display());
    println!("  version: {}", report.version);
    println!("  repo_root: {}", report.repo_root.display());
    println!("  cache_path: {}", report.cache_path.display());
    print_cache_diagnostics(report);
    println!(
        "  tokenizer: {} ({})",
        report.tokenizer_name, report.tokenizer_status
    );
    println!("  parsers:");

    for parser in &report.parsers {
        let status = if parser.available { "ok" } else { "missing" };
        println!("    {}: {} ({status})", parser.extension, parser.mode);
    }
}

fn print_cache_diagnostics(report: &DoctorReport) {
    println!("  cache:");
    println!("    size_bytes: {}", report.cache_size_bytes);
    println!("    entries: {}", report.cache.entry_count);
    println!("    stale_entries: {}", report.cache.stale_entry_count);
    print_cache_metadata(&report.cache);
}

fn print_cache_metadata(diagnostics: &CacheDiagnostics) {
    println!("    metadata:");
    let Some(metadata) = &diagnostics.metadata else {
        println!("      present: false");
        return;
    };

    println!("      present: true");
    println!("      respect_gitignore: {}", metadata.respect_gitignore);
    println!(
        "      max_file_bytes: {}",
        metadata
            .max_file_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_owned())
    );
    println!(
        "      include: {}",
        if metadata.include.is_empty() {
            "[]".to_owned()
        } else {
            metadata.include.join(", ")
        }
    );
    println!(
        "      exclude: {}",
        if metadata.exclude.is_empty() {
            "[]".to_owned()
        } else {
            metadata.exclude.join(", ")
        }
    );
}

fn format_doctor_json(report: &DoctorReport) -> String {
    let mut output = String::new();
    output.push_str("{\n");
    output.push_str("  \"binary\": \"");
    push_json_escaped(&mut output, &report.binary_path.display().to_string());
    output.push_str("\",\n  \"version\": \"");
    push_json_escaped(&mut output, report.version);
    output.push_str("\",\n  \"repo_root\": \"");
    push_json_escaped(&mut output, &report.repo_root.display().to_string());
    output.push_str("\",\n  \"cache_path\": \"");
    push_json_escaped(&mut output, &report.cache_path.display().to_string());
    output.push_str("\",\n  \"cache\": ");
    push_cache_diagnostics_json(&mut output, report);
    output.push_str(",\n  \"tokenizer\": {\"name\": \"");
    push_json_escaped(&mut output, &report.tokenizer_name);
    output.push_str("\", \"available\": ");
    output.push_str(if report.tokenizer_ok { "true" } else { "false" });
    output.push_str(", \"status\": \"");
    push_json_escaped(&mut output, &report.tokenizer_status);
    output.push_str("\"},\n  \"parsers\": [\n");

    for (index, parser) in report.parsers.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str("    {\"extension\": \"");
        push_json_escaped(&mut output, &parser.extension);
        output.push_str("\", \"mode\": \"");
        push_json_escaped(&mut output, parser.mode);
        output.push_str("\", \"available\": ");
        output.push_str(if parser.available { "true" } else { "false" });
        output.push('}');
    }

    output.push_str("\n  ]\n}");
    output
}

fn push_cache_diagnostics_json(output: &mut String, report: &DoctorReport) {
    output.push_str("{\"size_bytes\": ");
    output.push_str(&report.cache_size_bytes.to_string());
    output.push_str(", \"entries\": ");
    output.push_str(&report.cache.entry_count.to_string());
    output.push_str(", \"stale_entries\": ");
    output.push_str(&report.cache.stale_entry_count.to_string());
    output.push_str(", \"metadata\": ");
    push_cache_metadata_json(output, report.cache.metadata.as_ref());
    output.push('}');
}

fn push_cache_metadata_json(output: &mut String, metadata: Option<&CacheMetadata>) {
    let Some(metadata) = metadata else {
        output.push_str("null");
        return;
    };

    output.push_str("{\"respect_gitignore\": ");
    output.push_str(if metadata.respect_gitignore {
        "true"
    } else {
        "false"
    });
    output.push_str(", \"max_file_bytes\": ");
    match metadata.max_file_bytes {
        Some(value) => output.push_str(&value.to_string()),
        None => output.push_str("null"),
    }
    output.push_str(", \"include\": ");
    push_json_string_array(output, &metadata.include);
    output.push_str(", \"exclude\": ");
    push_json_string_array(output, &metadata.exclude);
    output.push('}');
}

fn push_json_string_array(output: &mut String, values: &[String]) {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push('"');
        push_json_escaped(output, value);
        output.push('"');
    }
    output.push(']');
}

fn push_json_escaped(output: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch.is_control() => {
                output.push_str("\\u");
                output.push_str(&format!("{:04x}", ch as u32));
            }
            _ => output.push(ch),
        }
    }
}

fn clear_cache(target: &Path) -> Result<()> {
    let root = fs::canonicalize(target)
        .with_context(|| format!("cannot resolve cache target {}", target.display()))?;
    let cache_path = cache_path_for_root(&root);

    match fs::remove_file(&cache_path) {
        Ok(()) => println!("cleared cache for {}", root.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("no cache for {}", root.display());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot remove cache {}", cache_path.display()));
        }
    }

    Ok(())
}

fn init_agent_files(
    target: &Path,
    force: bool,
    selection: AgentFileSelection,
    style: AgentInstructionStyle,
    max_tokens: Option<usize>,
    output_file: Option<&Path>,
) -> Result<()> {
    if max_tokens == Some(0) {
        bail!("--max-tokens must be greater than zero");
    }

    fs::create_dir_all(target)
        .with_context(|| format!("cannot create agent target {}", target.display()))?;

    let file_names = match selection {
        AgentFileSelection::Agents => vec!["AGENTS.md"],
        AgentFileSelection::Claude => vec!["CLAUDE.md"],
        AgentFileSelection::Both => vec!["AGENTS.md", "CLAUDE.md"],
    };
    let paths = file_names
        .iter()
        .map(|file_name| target.join(file_name))
        .collect::<Vec<_>>();

    if !force {
        let existing = paths
            .iter()
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();

        if !existing.is_empty() {
            bail!(
                "{} already exists; pass --force to overwrite",
                existing.join(" and ")
            );
        }
    }

    let output_name = output_file
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| DEFAULT_OUTPUT_FILE.to_owned());
    let command = agent_command(max_tokens, &output_name);
    for (file_name, path) in file_names.iter().zip(paths.iter()) {
        let (runner, title) = match *file_name {
            "AGENTS.md" => ("You run", "AGENTS.md"),
            "CLAUDE.md" => ("Claude runs", "CLAUDE.md"),
            _ => unreachable!("agent file selection only contains known files"),
        };
        let contents = agent_template(title, runner, &command, &output_name, style);
        write_agent_file(path, &contents)?;
    }

    println!("wrote {} in {}", file_names.join(" and "), target.display());
    Ok(())
}

fn write_agent_file(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))
}

fn agent_command(max_tokens: Option<usize>, output_file: &str) -> String {
    let mut command = String::from("bonsai .");
    if let Some(max_tokens) = max_tokens {
        command.push_str(&format!(" --max-tokens {max_tokens}"));
    }
    if output_file != DEFAULT_OUTPUT_FILE {
        command.push_str(" --output-file ");
        command.push_str(&shell_quote(output_file));
    }
    command
}

fn agent_template(
    title: &str,
    runner: &str,
    command: &str,
    output_file: &str,
    style: AgentInstructionStyle,
) -> String {
    match style {
        AgentInstructionStyle::Short => format!(
            "# {title}\n\nFor broad repository questions, run Bonsai first:\n\n```sh\n{command}\n```\n\nInspect `{output_file}` before answering.\n"
        ),
        AgentInstructionStyle::Detailed => format!(
            "# {title}\n\nFor repo-wide analysis, first run Bonsai. Use it for project summaries, architecture review, onboarding, broad bug hunting, and questions that need many files:\n\n```sh\n{command}\n```\n\nThen inspect `{output_file}` before answering.\n\nExpected behavior example:\n\n```text\nUser asks: summarize this whole project\n{runner}: {command}\nYou inspect: {output_file}\nThen answer from that context.\n```\n"
        ),
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/._-".contains(character))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn print_cli_reference() {
    let command = Cli::command();
    println!("# Bonsai CLI option reference\n");
    println!(
        "Generated from the Rust CLI schema. Regenerate this file with `scripts/generate-cli-reference.sh`.\n"
    );
    print_command_reference(&command, "bonsai");
}

fn print_command_reference(command: &clap::Command, command_path: &str) {
    let mut rendered_command = command.clone();
    println!("## `{command_path}`\n");
    println!("```text\n{}\n```", rendered_command.render_long_help());

    let subcommands = command
        .get_subcommands()
        .map(|subcommand| (subcommand.get_name().to_owned(), subcommand.clone()))
        .collect::<Vec<_>>();
    for (name, subcommand) in subcommands {
        let path = format!("{command_path} {name}");
        println!();
        print_command_reference(&subcommand, &path);
    }
}

fn max_file_bytes(cli: &Cli) -> Option<u64> {
    if cli.max_file_bytes == 0 {
        None
    } else {
        Some(cli.max_file_bytes)
    }
}

fn max_file_tokens(cli: &Cli) -> Option<usize> {
    cli.max_file_tokens.filter(|tokens| *tokens > 0)
}

/// Absolute path of the artifact Bonsai is about to write, so source scans
/// never ingest their own output (including unignored `bonsai.json` files)
/// or its numbered chunks. Clipboard runs write no file, so nothing is
/// excluded and a same-named source file stays visible.
fn output_exclusion(cli: &Cli) -> Option<PathBuf> {
    if !matches!(cli.output, OutputDestination::File) {
        return None;
    }
    Some(resolved_output_path(&cli.output_file))
}

fn resolved_output_path(output_file: &Path) -> PathBuf {
    if output_file.is_absolute() {
        return output_file.to_path_buf();
    }
    match env::current_dir()
        .ok()
        .and_then(|dir| fs::canonicalize(dir).ok())
    {
        Some(dir) => dir.join(output_file),
        None => output_file.to_path_buf(),
    }
}

fn validate_delta_options(cli: &Cli) -> Result<()> {
    if cli.changed_since.is_some() && (cli.incremental || cli.incremental_base.is_some()) {
        bail!(
            "choose one changed workflow: --preset changed/--incremental for the local cache, or --changed-since <git-ref> for a Git comparison"
        );
    }
    Ok(())
}

fn cache_metadata(cli: &Cli) -> CacheMetadata {
    CacheMetadata {
        include: cli.include.clone(),
        exclude: cli.exclude.clone(),
        respect_gitignore: cli.respect_gitignore,
        max_file_bytes: max_file_bytes(cli),
        exclude_generated: cli.exclude_generated,
    }
}

#[derive(Debug)]
enum IncrementalBase {
    Directory(PathBuf),
    Cache(ParseCache),
}

#[derive(Debug, Default)]
struct GitChanges {
    changed: HashMap<String, FileDelta>,
    deleted: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileDelta {
    Added,
    Changed,
    Unchanged,
}

#[derive(Debug, Default)]
struct IncrementalCounts {
    added: usize,
    changed: usize,
    unchanged: usize,
    skipped: usize,
    deleted: usize,
}

impl IncrementalCounts {
    fn record(&mut self, delta: FileDelta, included: bool) {
        match delta {
            FileDelta::Added => self.added += 1,
            FileDelta::Changed => self.changed += 1,
            FileDelta::Unchanged => self.unchanged += 1,
        }

        if !included {
            self.skipped += 1;
        }
    }
}

fn load_incremental_base(cli: &Cli) -> Result<Option<IncrementalBase>> {
    let Some(path) = &cli.incremental_base else {
        return Ok(None);
    };

    if path.is_dir() {
        let root = fs::canonicalize(path)
            .with_context(|| format!("cannot resolve incremental base {}", path.display()))?;
        return Ok(Some(IncrementalBase::Directory(root)));
    }

    Ok(Some(IncrementalBase::Cache(ParseCache::load_required(
        path.clone(),
    )?)))
}

fn load_git_changes(cli: &Cli, root: &Path) -> Result<Option<GitChanges>> {
    let Some(git_ref) = &cli.changed_since else {
        return Ok(None);
    };

    let diff_output = ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--name-status")
        .arg("-z")
        .arg("--relative")
        .arg(git_ref)
        .arg("--")
        .output()
        .with_context(|| format!("cannot run git diff against {git_ref}"))?;

    if !diff_output.status.success() {
        let stderr = String::from_utf8_lossy(&diff_output.stderr);
        bail!("git diff against {git_ref} failed: {}", stderr.trim());
    }

    let mut changes = parse_git_diff_changes(&diff_output.stdout, root, cli)?;

    let mut command = ProcessCommand::new("git");
    command
        .arg("-C")
        .arg(root)
        .arg("ls-files")
        .arg("--others")
        .arg("-z");
    if cli.respect_gitignore {
        command.arg("--exclude-standard");
    }
    command.arg("--");

    let untracked_output = command
        .output()
        .context("cannot list untracked git files")?;
    if !untracked_output.status.success() {
        let stderr = String::from_utf8_lossy(&untracked_output.stderr);
        bail!("cannot list untracked git files: {}", stderr.trim());
    }

    add_untracked_git_changes(&mut changes, &untracked_output.stdout, root, cli)?;
    Ok(Some(changes))
}

fn parse_git_diff_changes(bytes: &[u8], root: &Path, cli: &Cli) -> Result<GitChanges> {
    let fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect::<Vec<_>>();
    let mut changes = GitChanges::default();
    let mut index = 0;

    while index < fields.len() {
        let status = fields[index].as_str();
        index += 1;

        if status.starts_with('R') || status.starts_with('C') {
            if index + 1 >= fields.len() {
                bail!("invalid git diff --name-status output");
            }
            let old_path = normalize_relative_path(&fields[index]);
            let new_path = normalize_relative_path(&fields[index + 1]);
            index += 2;

            if status.starts_with('R') && git_path_allowed(root, &old_path, cli)? {
                changes.deleted.push(old_path);
            }
            if git_path_allowed(root, &new_path, cli)? {
                changes.changed.insert(new_path, FileDelta::Added);
            }
            continue;
        }

        if index >= fields.len() {
            bail!("invalid git diff --name-status output");
        }
        let path = normalize_relative_path(&fields[index]);
        index += 1;

        if !git_path_allowed(root, &path, cli)? {
            continue;
        }

        if status.starts_with('D') {
            changes.deleted.push(path);
        } else if status.starts_with('A') {
            changes.changed.insert(path, FileDelta::Added);
        } else {
            changes.changed.insert(path, FileDelta::Changed);
        }
    }

    changes.deleted.sort();
    changes.deleted.dedup();
    Ok(changes)
}

fn add_untracked_git_changes(
    changes: &mut GitChanges,
    bytes: &[u8],
    root: &Path,
    cli: &Cli,
) -> Result<()> {
    for field in bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
    {
        let path = normalize_relative_path(&String::from_utf8_lossy(field));
        if git_path_allowed(root, &path, cli)? {
            changes.changed.insert(path, FileDelta::Added);
        }
    }

    Ok(())
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn git_path_allowed(root: &Path, relative_path: &str, cli: &Cli) -> Result<bool> {
    let path = root.join(relative_path);
    Ok(is_supported_path(&path) && matches_path_filters(root, &path, &cli.include, &cli.exclude)?)
}

fn classify_file(
    incremental_base: &Option<IncrementalBase>,
    git_changes: &Option<GitChanges>,
    parse_cache: &ParseCache,
    baseline_metadata_matches: bool,
    path: &Path,
    relative_path: &str,
    metadata: &fs::Metadata,
) -> Result<FileDelta> {
    if let Some(git_changes) = git_changes {
        return Ok(git_changes
            .changed
            .get(relative_path)
            .copied()
            .unwrap_or(FileDelta::Unchanged));
    }

    match incremental_base {
        Some(IncrementalBase::Directory(base_root)) => {
            let base_path = base_root.join(relative_path);
            directory_file_delta(&base_path, path, metadata)
        }
        Some(IncrementalBase::Cache(base_cache)) => {
            if !baseline_metadata_matches {
                return Ok(FileDelta::Added);
            }
            Ok(cache_status_to_delta(base_cache.status(path, metadata)))
        }
        None => {
            if !baseline_metadata_matches {
                return Ok(FileDelta::Added);
            }
            Ok(cache_status_to_delta(parse_cache.status(path, metadata)))
        }
    }
}

fn baseline_metadata_matches(
    incremental_base: &Option<IncrementalBase>,
    parse_cache: &ParseCache,
    metadata: &CacheMetadata,
) -> bool {
    match incremental_base {
        Some(IncrementalBase::Cache(base_cache)) => base_cache.metadata_matches(metadata),
        Some(IncrementalBase::Directory(_)) => true,
        None => parse_cache.metadata_matches(metadata),
    }
}

fn should_include_delta(
    cli: &Cli,
    incremental_base: &Option<IncrementalBase>,
    delta: FileDelta,
) -> bool {
    if cli.incremental || cli.changed_since.is_some() || incremental_base.is_some() {
        delta != FileDelta::Unchanged
    } else {
        true
    }
}

fn directory_file_delta(
    base_path: &Path,
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<FileDelta> {
    if !base_path.exists() {
        return Ok(FileDelta::Added);
    }

    if same_file_contents(base_path, path, metadata)? {
        Ok(FileDelta::Unchanged)
    } else {
        Ok(FileDelta::Changed)
    }
}

fn same_file_contents(base_path: &Path, path: &Path, metadata: &fs::Metadata) -> Result<bool> {
    let Ok(base_metadata) = fs::metadata(base_path) else {
        return Ok(false);
    };

    if !base_metadata.is_file() || base_metadata.len() != metadata.len() {
        return Ok(false);
    }

    let base_bytes = fs::read(base_path)
        .with_context(|| format!("cannot read incremental base file {}", base_path.display()))?;
    let current_bytes =
        fs::read(path).with_context(|| format!("cannot read source file {}", path.display()))?;
    Ok(base_bytes == current_bytes)
}

fn cache_status_to_delta(status: CacheStatus) -> FileDelta {
    match status {
        CacheStatus::Added => FileDelta::Added,
        CacheStatus::Changed => FileDelta::Changed,
        CacheStatus::Unchanged => FileDelta::Unchanged,
    }
}

fn relative_path_set(root: &Path, paths: &[PathBuf]) -> HashSet<String> {
    paths
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn deleted_files(
    cli: &Cli,
    incremental_base: &Option<IncrementalBase>,
    git_changes: &Option<GitChanges>,
    parse_cache: &ParseCache,
    baseline_metadata_matches: bool,
    root: &Path,
    current_relative_paths: &HashSet<String>,
) -> Result<Vec<String>> {
    if let Some(git_changes) = git_changes {
        return Ok(git_changes.deleted.clone());
    }

    match incremental_base {
        Some(IncrementalBase::Directory(base_root)) => {
            let base_paths = collect_code_files(
                base_root,
                &WalkerOptions {
                    include: cli.include.clone(),
                    exclude: cli.exclude.clone(),
                    respect_gitignore: cli.respect_gitignore,
                    max_file_bytes: max_file_bytes(cli),
                    exclude_generated: cli.exclude_generated,
                    output_file: output_exclusion(cli),
                },
            )?;
            let mut deleted = relative_path_set(base_root, &base_paths)
                .difference(current_relative_paths)
                .cloned()
                .collect::<Vec<_>>();
            deleted.sort();
            Ok(deleted)
        }
        Some(IncrementalBase::Cache(base_cache)) if baseline_metadata_matches => {
            Ok(base_cache.deleted_paths(root))
        }
        Some(IncrementalBase::Cache(_)) => Ok(Vec::new()),
        None if cli.incremental && baseline_metadata_matches => Ok(parse_cache.deleted_paths(root)),
        None => Ok(Vec::new()),
    }
}

#[derive(Debug)]
struct RunStats {
    output_target: String,
    files_scanned: usize,
    files_dropped: usize,
    selected_level: CompressionLevel,
    max_tokens: usize,
    tokenizer: TokenizerKind,
    raw_tokens: Option<usize>,
    shrunk_tokens: usize,
    tokens_saved: Option<usize>,
    saving_percent: Option<f64>,
    over_budget: bool,
}

impl RunStats {
    fn new(
        cli: &Cli,
        selected_level: CompressionLevel,
        files_scanned: usize,
        files_dropped: usize,
        raw_context: Option<&str>,
        shrunk_tokens: usize,
        counter: &TokenCounter,
    ) -> Result<Self> {
        let raw_tokens = raw_context.map(|context| count_text_tokens(context, counter));
        let tokens_saved = raw_tokens.map(|tokens| tokens.saturating_sub(shrunk_tokens));
        let saving_percent = raw_tokens.map(|tokens| {
            if tokens == 0 {
                0.0
            } else {
                tokens_saved.unwrap_or(0) as f64 / tokens as f64 * 100.0
            }
        });

        Ok(Self {
            output_target: output_target(cli),
            files_scanned,
            files_dropped,
            selected_level,
            max_tokens: cli.max_tokens,
            tokenizer: counter.tokenizer(),
            raw_tokens,
            shrunk_tokens,
            tokens_saved,
            saving_percent,
            over_budget: shrunk_tokens > cli.max_tokens,
        })
    }
}

fn full_context_files(
    files: &[ProcessedFile],
    counter: &TokenCounter,
) -> Result<Vec<ProcessedFile>> {
    files
        .iter()
        .cloned()
        .map(|mut file| {
            file.level = CompressionLevel::Full;
            // The full variant is never rewritten by token capping (level 1
            // rejects caps), so raw full source is complete by construction.
            file.truncated = false;
            file.token_count = count_text_tokens(file.content(), counter);
            Ok(file)
        })
        .collect()
}

fn format_context(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
    cli: &Cli,
    deleted_files: &[String],
    dropped_paths: &[String],
) -> String {
    let options = format_options(files, cli, deleted_files, dropped_paths, &[]);
    match cli.format {
        OutputFormat::Json => format_repository_context_json(files, metadata, &options),
        OutputFormat::Text => format_repository_context_text(files, metadata, &options),
        OutputFormat::Xml => format_repository_context_xml(files, metadata, &options),
    }
}

fn format_options(
    files: &[ProcessedFile],
    cli: &Cli,
    deleted_files: &[String],
    dropped_paths: &[String],
    extra_warnings: &[String],
) -> FormatOptions {
    let map_only_fallback = uses_map_only_fallback(cli);
    let mut warnings = content_warnings(files, dropped_paths);
    warnings.extend(extra_warnings.iter().cloned());
    FormatOptions {
        project_map_only: cli.project_map_only,
        project_map_mode: cli.project_map.into(),
        include_file_hashes: cli.file_hashes,
        include_token_counts: !cli.no_token_counts,
        include_files: !cli.project_map_only && !cli.no_content && !map_only_fallback,
        include_content: !cli.project_map_only && !cli.no_content && !map_only_fallback,
        deleted_files: if cli.project_map_only || map_only_fallback {
            Vec::new()
        } else {
            deleted_files.to_vec()
        },
        directory_summaries: if (cli.directory_summaries || map_only_fallback)
            && !cli.project_map_only
        {
            build_directory_summaries(files)
        } else {
            Vec::new()
        },
        warnings,
    }
}

/// Fidelity warnings embedded in the generated context so agents never mistake
/// lossy output for complete evidence. Empty files are ignored: they lose
/// nothing to summarization.
fn content_warnings(files: &[ProcessedFile], dropped_paths: &[String]) -> Vec<String> {
    let mut warnings = Vec::new();

    let mut summaries: Vec<&str> = files
        .iter()
        .filter(|file| file.level == CompressionLevel::TreeMap && !file.content().is_empty())
        .map(|file| file.path.as_str())
        .collect();
    summaries.sort_unstable();
    if !summaries.is_empty() {
        warnings.push(tree_map_warning(summaries.len(), &summaries));
    }

    let mut cut: Vec<&str> = files
        .iter()
        .filter(|file| file.truncated)
        .map(|file| file.path.as_str())
        .collect();
    cut.sort_unstable();
    if !cut.is_empty() {
        warnings.push(truncated_warning(cut.len(), &cut));
    }

    if !dropped_paths.is_empty() {
        let mut dropped: Vec<&str> = dropped_paths.iter().map(String::as_str).collect();
        dropped.sort_unstable();
        warnings.push(dropped_warning(dropped.len(), &dropped));
    }

    warnings
}

fn tree_map_warning(count: usize, paths: &[&str]) -> String {
    format!(
        "{} file(s) are level-3 tree-map summaries (names only, no implementation bodies) and must not be treated as evidence of behavior: {}.",
        count,
        capped_path_list(paths)
    )
}

fn truncated_warning(count: usize, paths: &[&str]) -> String {
    format!(
        "{} file(s) were cut mid-content to fit the token budget; their content ends with ... and is incomplete: {}.",
        count,
        capped_path_list(paths)
    )
}

fn dropped_warning(count: usize, paths: &[&str]) -> String {
    format!(
        "{} file(s) were omitted to fit the token budget: {}.",
        count,
        capped_path_list(paths)
    )
}

fn over_budget_warning(output_tokens: usize, max_tokens: usize) -> String {
    format!(
        "Output is {output_tokens} tokens, above max-tokens {max_tokens} even after maximum compression; treat this context as incomplete."
    )
}

fn capped_path_list(paths: &[&str]) -> String {
    const SHOWN: usize = 10;
    let mut listed = paths
        .iter()
        .take(SHOWN)
        .map(|path| (*path).to_owned())
        .collect::<Vec<_>>()
        .join(", ");
    if paths.len() > SHOWN {
        listed.push_str(&format!(" (and {} more)", paths.len() - SHOWN));
    }
    listed
}

/// Warnings for the finished run: content fidelity plus the over-budget notice
/// when the final output still exceeds the requested budget.
fn final_warnings(
    files: &[ProcessedFile],
    dropped_paths: &[String],
    output_tokens: usize,
    max_tokens: usize,
) -> Vec<String> {
    let mut warnings = content_warnings(files, dropped_paths);
    if output_tokens > max_tokens {
        warnings.push(over_budget_warning(output_tokens, max_tokens));
    }
    warnings
}

fn uses_map_only_fallback(cli: &Cli) -> bool {
    cli.map_only_under
        .is_some_and(|tokens| cli.max_tokens < tokens)
}

fn set_files_to_tree_map(files: &mut [ProcessedFile], counter: &TokenCounter) {
    for file in files {
        file.level = CompressionLevel::TreeMap;
        file.token_count = count_text_tokens(file.content(), counter);
    }
}

fn fit_formatted_context(
    mut files: Vec<ProcessedFile>,
    metadata: &RepositoryMetadata,
    cli: &Cli,
    counter: &TokenCounter,
    deleted_files: &[String],
) -> Result<(Vec<ProcessedFile>, String, usize, Vec<String>)> {
    let mut dropped_paths: Vec<String> = Vec::new();

    loop {
        sort_files(&mut files, cli.sort);
        let mut current_metadata = metadata.clone();
        current_metadata.file_count = files.len();
        let context = maybe_wrap_prompt(
            format_context(
                &files,
                &current_metadata,
                cli,
                deleted_files,
                &dropped_paths,
            ),
            cli,
        );
        let output_tokens = count_text_tokens(&context, counter);

        if output_tokens <= cli.max_tokens {
            return Ok((files, context, output_tokens, dropped_paths));
        }

        if cli.level == 1 {
            bail!("output needs {output_tokens} tokens, above --max-tokens {}; level 1 preserves full source. Increase --max-tokens, select fewer files, or explicitly choose --level 2 or 3 for lossy compression. Output was not written", cli.max_tokens);
        }

        if downgrade_largest_file(&mut files, counter) {
            continue;
        }

        if cli.drop_low_priority {
            if let Some(dropped) = drop_lowest_priority_file(&mut files) {
                dropped_paths.push(dropped);
                continue;
            }
        }

        // Still over budget with nothing left to shrink: label the output as
        // incomplete evidence instead of returning it silently. The embedded
        // token number must describe the bytes actually emitted, so stabilize
        // the warning text against the final recount.
        return Ok(finalize_over_budget_context(
            files,
            metadata,
            cli,
            counter,
            deleted_files,
            dropped_paths,
            output_tokens,
        ));
    }
}

/// Render the final document with the given extra warnings applied.
fn render_final_context(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
    cli: &Cli,
    deleted_files: &[String],
    dropped_paths: &[String],
    extra_warnings: &[String],
) -> String {
    let options = format_options(files, cli, deleted_files, dropped_paths, extra_warnings);
    let mut final_metadata = metadata.clone();
    final_metadata.file_count = files.len();
    maybe_wrap_prompt(
        match cli.format {
            OutputFormat::Json => format_repository_context_json(files, &final_metadata, &options),
            OutputFormat::Text => format_repository_context_text(files, &final_metadata, &options),
            OutputFormat::Xml => format_repository_context_xml(files, &final_metadata, &options),
        },
        cli,
    )
}

/// Emit over-budget output whose embedded warning reports the final verified
/// token count. Adding the warning itself costs tokens, so reformat until the
/// reported number matches the recount (digit-width changes converge in one
/// or two passes; five attempts bound pathological growth).
fn finalize_over_budget_context(
    files: Vec<ProcessedFile>,
    metadata: &RepositoryMetadata,
    cli: &Cli,
    counter: &TokenCounter,
    deleted_files: &[String],
    dropped_paths: Vec<String>,
    initial_tokens: usize,
) -> (Vec<ProcessedFile>, String, usize, Vec<String>) {
    let mut reported = initial_tokens;
    let mut context = String::new();
    let mut output_tokens = initial_tokens;

    for _ in 0..5 {
        let extra = vec![over_budget_warning(reported, cli.max_tokens)];
        context =
            render_final_context(&files, metadata, cli, deleted_files, &dropped_paths, &extra);
        output_tokens = count_text_tokens(&context, counter);
        if output_tokens == reported {
            break;
        }
        reported = output_tokens;
    }

    (files, context, output_tokens, dropped_paths)
}

fn drop_lowest_priority_file(files: &mut Vec<ProcessedFile>) -> Option<String> {
    if files.len() <= 1 {
        return None;
    }

    let Some(index) = files
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            effective_priority_score(left)
                .cmp(&effective_priority_score(right))
                .then(right.token_count.cmp(&left.token_count))
                .then(right.path.cmp(&left.path))
        })
        .map(|(index, _)| index)
    else {
        return None;
    };

    Some(files.remove(index).path)
}

fn reserved_content_budget(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
    cli: &Cli,
    counter: &TokenCounter,
    deleted_files: &[String],
) -> Result<usize> {
    let overhead_files = files
        .iter()
        .map(|file| {
            let mut overhead_file = ProcessedFile::new(
                file.path.clone(),
                file.level,
                parser::FileVariants {
                    full: None,
                    skeleton: String::new(),
                    tree_map: String::new(),
                },
            );
            overhead_file.token_count = 0;
            overhead_file.content_hash = file.content_hash.clone();
            overhead_file
        })
        .collect::<Vec<_>>();
    let overhead = maybe_wrap_prompt(
        format_context(&overhead_files, metadata, cli, deleted_files, &[]),
        cli,
    );
    let overhead_tokens = count_text_tokens(&overhead, counter);

    Ok(cli.max_tokens.saturating_sub(overhead_tokens))
}

fn sort_files(files: &mut [ProcessedFile], sort: SortMode) {
    match sort {
        SortMode::Path => files.sort_by(|left, right| left.path.cmp(&right.path)),
        SortMode::Tokens => files.sort_by(|left, right| {
            right
                .token_count
                .cmp(&left.token_count)
                .then(left.path.cmp(&right.path))
        }),
        SortMode::Priority => files.sort_by(|left, right| {
            effective_priority_score(right)
                .cmp(&effective_priority_score(left))
                .then(left.path.cmp(&right.path))
        }),
    }
}

fn stable_content_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn build_directory_summaries(files: &[ProcessedFile]) -> Vec<DirectorySummary> {
    let mut by_dir: BTreeMap<String, DirectorySummary> = BTreeMap::new();

    for file in files {
        let directory = file
            .path
            .rsplit_once('/')
            .map(|(directory, _)| directory)
            .unwrap_or(".");
        let entry = by_dir
            .entry(directory.to_owned())
            .or_insert_with(|| DirectorySummary {
                path: directory.to_owned(),
                file_count: 0,
                tokens: 0,
            });
        entry.file_count += 1;
        entry.tokens += file.token_count;
    }

    by_dir.into_values().collect()
}

fn maybe_wrap_prompt(context: String, cli: &Cli) -> String {
    if !cli.prompt && cli.ask_template.is_none() {
        return context;
    }

    let format_name = match cli.format {
        OutputFormat::Json => "JSON",
        OutputFormat::Text => "text",
        OutputFormat::Xml => "XML",
    };
    let task = cli.ask_template.as_deref().unwrap_or(
        "Use this repo context to explain the architecture, identify the main entry points, and tell me where to start reading.",
    );

    format!(
        "{task}\n\nThe context below is compressed Bonsai {format_name}. Use it as the source of truth before answering.\n\n<context>\n{context}</context>\n"
    )
}

fn generated_at_unix() -> Result<String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?;
    Ok(duration.as_secs().to_string())
}

fn output_target(cli: &Cli) -> String {
    if cli.dry_run {
        return "dry-run".to_owned();
    }

    match cli.output {
        OutputDestination::Clipboard => "clipboard".to_owned(),
        OutputDestination::File => cli.output_file.display().to_string(),
    }
}

fn prepare_output_path(path: &Path) -> Result<()> {
    if path.exists() && !path.is_file() {
        bail!("output path exists but is not a file: {}", path.display());
    }

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create output directory {}", parent.display()))?;
    }

    Ok(())
}

fn write_output_file(path: &Path, context: &str, quiet: bool) -> Result<()> {
    let output_exists = path.is_file();
    prepare_output_path(path)?;

    if output_exists && !quiet {
        eprintln!("warning: replacing existing output file {}", path.display());
    }

    fs::write(path, context).with_context(|| format!("cannot write {}", path.display()))
}

fn print_success(stats: &RunStats) {
    if stats.over_budget {
        match stats.output_target.as_str() {
            "clipboard" => println!(
                "Bonsai copied context to the clipboard ({} files, {} / {} tokens): over budget even after maximum compression; treat this context as incomplete.",
                stats.files_scanned, stats.shrunk_tokens, stats.max_tokens
            ),
            output => println!(
                "Bonsai wrote {} ({} files, {} / {} tokens): over budget even after maximum compression; treat this context as incomplete.",
                output, stats.files_scanned, stats.shrunk_tokens, stats.max_tokens
            ),
        }
        return;
    }

    match stats.output_target.as_str() {
        "clipboard" => println!(
            "Bonsai copied context to the clipboard ({} files, {} tokens).",
            stats.files_scanned, stats.shrunk_tokens
        ),
        output => println!(
            "Bonsai wrote {} ({} files, {} tokens).",
            output, stats.files_scanned, stats.shrunk_tokens
        ),
    }
}

fn handle_empty_selection(cli: &Cli, root: &std::path::Path) -> Result<()> {
    let supported = supported_extensions()
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(" ");
    let message = format!(
        "no supported files found under {}. Supported extensions: {supported}. Check --include, --exclude, and --no-respect-gitignore.",
        root.display()
    );

    if cli.fail_on_empty {
        bail!("{message}");
    }

    if !cli.quiet {
        eprintln!("warning: {message}");
    }
    Ok(())
}

fn print_selected_files(root: &std::path::Path, paths: &[PathBuf]) {
    for path in paths {
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        println!("{relative_path}");
    }
}

fn print_summary(stats: &RunStats) {
    println!("summary:");
    println!("  output: {}", stats.output_target);
    println!("  files_included: {}", stats.files_scanned);
    println!("  files_dropped: {}", stats.files_dropped);
    println!("  selected_level: {}", stats.selected_level.as_u8());
    println!("  tokenizer: {}", stats.tokenizer.as_str());
    println!(
        "  output_tokens: {} / {}",
        stats.shrunk_tokens, stats.max_tokens
    );
    println!("  over_budget: {}", stats.over_budget);
    if stats.over_budget {
        println!(
            "  budget_note: output is {} tokens, above max-tokens {} even after maximum compression",
            stats.shrunk_tokens, stats.max_tokens
        );
    }
}

fn print_stats(stats: &RunStats) {
    println!("stats:");
    println!("  raw_tokens: {}", stats.raw_tokens.unwrap_or(0));
    println!("  shrunk_tokens: {}", stats.shrunk_tokens);
    println!("  tokens_saved: {}", stats.tokens_saved.unwrap_or(0));
    println!(
        "  saving_percent: {:.2}",
        stats.saving_percent.unwrap_or(0.0)
    );
    println!("  files_scanned: {}", stats.files_scanned);
    println!("  files_dropped: {}", stats.files_dropped);
}

fn print_incremental_summary(counts: &IncrementalCounts) {
    println!("incremental_summary:");
    println!("  added: {}", counts.added);
    println!("  changed: {}", counts.changed);
    println!("  unchanged: {}", counts.unchanged);
    println!("  skipped: {}", counts.skipped);
    println!("  deleted: {}", counts.deleted);
}

fn print_dry_run(
    files: &[ProcessedFile],
    deleted_files: &[String],
    estimated_tokens: usize,
    max_tokens: usize,
    warnings: &[String],
) {
    println!("dry_run:");
    println!("  files: {}", files.len());
    println!("  deleted: {}", deleted_files.len());
    println!("  estimated_tokens: {estimated_tokens}");
    println!("  max_tokens: {max_tokens}");
    if !warnings.is_empty() {
        println!("  warnings:");
        for warning in warnings {
            println!("    - {warning}");
        }
    }
    println!("selected_files:");
    for file in files {
        println!(
            "  {}  L{}  {} tokens",
            file.path,
            file.level.as_u8(),
            file.token_count
        );
    }

    if !deleted_files.is_empty() {
        println!("deleted_files:");
        for path in deleted_files {
            println!("  {path}");
        }
    }
}

fn print_detailed_stats(files: &[ProcessedFile], _stats: &RunStats) {
    println!("detailed_stats:");
    println!("  files_reported: {}", files.len());

    println!("  per_file_tokens:");
    for f in files {
        println!("    {}: {}", f.path, f.token_count);
    }

    let mut ext_map: HashMap<String, usize> = HashMap::new();
    for f in files {
        let ext = std::path::Path::new(&f.path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .unwrap_or_else(|| "<noext>".to_string());
        *ext_map.entry(ext).or_default() += f.token_count;
    }

    let mut ext_vec: Vec<_> = ext_map.into_iter().collect();
    ext_vec.sort_by(|a, b| b.1.cmp(&a.1));
    println!("  tokens_by_extension:");
    for (k, v) in ext_vec {
        println!("    {}: {}", k, v);
    }

    let mut toks: Vec<usize> = files.iter().map(|f| f.token_count).collect();
    toks.sort();
    let mn = toks.first().cloned().unwrap_or(0);
    let mx = toks.last().cloned().unwrap_or(0);
    let mean = if toks.is_empty() {
        0.0
    } else {
        toks.iter().sum::<usize>() as f64 / toks.len() as f64
    };
    let median = if toks.is_empty() {
        0.0
    } else if toks.len() % 2 == 1 {
        toks[toks.len() / 2] as f64
    } else {
        (toks[toks.len() / 2 - 1] + toks[toks.len() / 2]) as f64 / 2.0
    };
    println!(
        "  token_distribution: min={}, median={:.1}, mean={:.1}, max={}",
        mn, median, mean, mx
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use parser::FileVariants;

    #[test]
    fn xml_output_tokens_match_final_document() {
        assert_final_token_count_matches(OutputFormat::Xml);
    }

    #[test]
    fn json_output_tokens_match_final_document() {
        assert_final_token_count_matches(OutputFormat::Json);
    }

    #[test]
    fn text_output_tokens_match_final_document() {
        assert_final_token_count_matches(OutputFormat::Text);
    }

    #[test]
    fn drop_low_priority_prunes_after_tree_map() {
        let mut cli = test_cli(OutputFormat::Xml);
        cli.max_tokens = 115;
        cli.drop_low_priority = true;
        let counter = TokenCounter::new(cli.tokenizer).unwrap();
        let metadata = RepositoryMetadata {
            generated_at: "1234567890".to_owned(),
            repo_root: "/tmp/demo".to_owned(),
            max_tokens: cli.max_tokens,
            compression_level: 2,
            file_count: 2,
        };
        let files = vec![
            ProcessedFile::new(
                "README.md".to_owned(),
                CompressionLevel::TreeMap,
                FileVariants {
                    full: None,
                    skeleton: "README".to_owned(),
                    tree_map: "README".to_owned(),
                },
            ),
            ProcessedFile::new(
                "src/generated/deep/file.rs".to_owned(),
                CompressionLevel::TreeMap,
                FileVariants {
                    full: None,
                    skeleton: "generated".to_owned(),
                    tree_map: "generated".to_owned(),
                },
            ),
        ];

        let (files, context, output_tokens, dropped) =
            fit_formatted_context(files, &metadata, &cli, &counter, &[]).unwrap();

        assert_eq!(dropped, vec!["src/generated/deep/file.rs".to_owned()]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "README.md");
        assert!(context.contains("file_count=\"1\""));
        assert!(!context.contains("<entry path=\"src/generated/deep/file.rs\""));
        assert!(!context.contains("<file path=\"src/generated/deep/file.rs\""));
        assert!(context.contains("were omitted to fit the token budget"));
        assert!(context.contains("tree-map summaries"));
        // The fidelity warnings themselves push this tiny budget over the
        // limit, so the output must also carry the over-budget notice.
        assert!(output_tokens > cli.max_tokens);
        assert!(context.contains("above max-tokens"));
    }

    #[test]
    fn map_only_under_outputs_map_and_directory_summaries_without_files() {
        let mut cli = test_cli(OutputFormat::Xml);
        cli.max_tokens = 100;
        cli.map_only_under = Some(500);
        let counter = TokenCounter::new(cli.tokenizer).unwrap();
        let metadata = RepositoryMetadata {
            generated_at: "1234567890".to_owned(),
            repo_root: "/tmp/demo".to_owned(),
            max_tokens: cli.max_tokens,
            compression_level: 2,
            file_count: 2,
        };
        let mut files = vec![
            ProcessedFile::new(
                "Cargo.toml".to_owned(),
                CompressionLevel::Skeleton,
                FileVariants {
                    full: None,
                    skeleton: "name = \"demo\"".to_owned(),
                    tree_map: "Cargo manifest".to_owned(),
                },
            ),
            ProcessedFile::new(
                "src/lib.rs".to_owned(),
                CompressionLevel::Skeleton,
                FileVariants {
                    full: None,
                    skeleton: "pub fn greet() { ... }".to_owned(),
                    tree_map: "pub fn greet()".to_owned(),
                },
            ),
        ];
        set_files_to_tree_map(&mut files, &counter);

        let (_files, context, output_tokens, _dropped) =
            fit_formatted_context(files, &metadata, &cli, &counter, &["old.rs".to_owned()])
                .unwrap();

        assert!(output_tokens > 0);
        assert!(context.contains("<project_map>"));
        assert!(context.contains("<directory_summaries>"));
        assert!(context.contains("<directory path=\"src\""));
        assert!(!context.contains("<files>"));
        assert!(!context.contains("<deleted_files>"));
        assert!(context.contains("level=\"3\""));
    }

    #[test]
    fn content_warnings_name_tree_map_truncated_and_dropped_files() {
        let mut tree_map = ProcessedFile::new(
            "src/map.rs".to_owned(),
            CompressionLevel::TreeMap,
            FileVariants {
                full: None,
                skeleton: "skeleton".to_owned(),
                tree_map: "fn map()".to_owned(),
            },
        );
        tree_map.token_count = 3;
        let mut cut = ProcessedFile::new(
            "src/cut.rs".to_owned(),
            CompressionLevel::Skeleton,
            FileVariants {
                full: None,
                skeleton: "skeleton...".to_owned(),
                tree_map: "cut".to_owned(),
            },
        );
        cut.token_count = 2;
        cut.truncated = true;
        let files = vec![tree_map, cut];

        let warnings = content_warnings(&files, &["src/gone.rs".to_owned()]);

        assert_eq!(warnings.len(), 3);
        assert!(warnings[0].contains("tree-map summaries"));
        assert!(warnings[0].contains("src/map.rs"));
        assert!(warnings[0].contains("must not be treated as evidence of behavior"));
        assert!(warnings[1].contains("cut mid-content"));
        assert!(warnings[1].contains("src/cut.rs"));
        assert!(warnings[2].contains("were omitted"));
        assert!(warnings[2].contains("src/gone.rs"));
    }

    #[test]
    fn content_warnings_are_empty_for_complete_context() {
        let mut file = ProcessedFile::new(
            "src/lib.rs".to_owned(),
            CompressionLevel::Full,
            FileVariants {
                full: Some("pub fn lib() {}".to_owned()),
                skeleton: "pub fn lib() { ... }".to_owned(),
                tree_map: "pub fn lib()".to_owned(),
            },
        );
        file.token_count = 5;

        assert!(content_warnings(&[file], &[]).is_empty());
    }

    #[test]
    fn content_warnings_cap_long_path_lists() {
        let files: Vec<ProcessedFile> = (0..12)
            .map(|index| {
                let mut file = ProcessedFile::new(
                    format!("src/file{index:02}.rs"),
                    CompressionLevel::TreeMap,
                    FileVariants {
                        full: None,
                        skeleton: String::new(),
                        tree_map: format!("file{index:02}"),
                    },
                );
                file.token_count = 1;
                file
            })
            .collect();

        let warnings = content_warnings(&files, &[]);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("12 file(s)"));
        assert!(warnings[0].contains("(and 2 more)"));
    }

    fn assert_final_token_count_matches(format: OutputFormat) {
        let cli = test_cli(format);
        let counter = TokenCounter::new(cli.tokenizer).unwrap();
        let metadata = RepositoryMetadata {
            generated_at: "1234567890".to_owned(),
            repo_root: "/tmp/demo".to_owned(),
            max_tokens: 12000,
            compression_level: 2,
            file_count: 1,
        };
        let files = vec![ProcessedFile::new(
            "src/lib.rs".to_owned(),
            CompressionLevel::Skeleton,
            FileVariants {
                full: Some("pub fn greet() { println!(\"hello\"); }".to_owned()),
                skeleton: "pub fn greet() { ... }".to_owned(),
                tree_map: "pub fn greet()".to_owned(),
            },
        )];

        let (_files, context, output_tokens, _dropped) =
            fit_formatted_context(files, &metadata, &cli, &counter, &[]).unwrap();

        assert_eq!(output_tokens, count_text_tokens(&context, &counter));
    }

    fn test_cli(format: OutputFormat) -> Cli {
        Cli {
            command: None,
            path: PathBuf::from("."),
            config: None,
            preset: None,
            max_tokens: 12000,
            tokenizer: TokenizerKind::default(),
            max_file_bytes: 1_048_576,
            max_file_tokens: None,
            level: 2,
            output: OutputDestination::File,
            output_file: PathBuf::from("bonsai.xml"),
            format,
            project_map_only: false,
            map_only_under: None,
            project_map: ProjectMapMode::Flat,
            file_hashes: false,
            no_token_counts: false,
            no_content: false,
            dry_run: false,
            sort: SortMode::Path,
            directory_summaries: false,
            fail_over_budget: false,
            drop_low_priority: false,
            incremental: false,
            incremental_base: None,
            changed_since: None,
            incremental_summary: false,
            include: Vec::new(),
            exclude: Vec::new(),
            respect_gitignore: true,
            exclude_generated: false,
            focus: None,
            print_files: false,
            fail_on_empty: false,
            quiet: false,
            stats: false,
            detailed_stats: false,
            summary: false,
            prompt: false,
            ask_template: None,
        }
    }
}
