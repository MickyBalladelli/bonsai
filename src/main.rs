mod budget;
mod cache;
mod formatter;
mod parser;
mod walker;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser, ValueEnum};

use budget::{
    count_text_tokens, downgrade_largest_file, file_priority_score, optimize_budget, ProcessedFile,
    TokenCounter, TokenizerKind,
};
use cache::{cache_path_for_root, CacheMetadata, CacheStatus, ParseCache};
use formatter::{
    format_repository_context_json, format_repository_context_xml, DirectorySummary, FormatOptions,
    RepositoryMetadata,
};
use parser::{compress_file, parser_support_for_extension, CompressionLevel, ParserMode};
use walker::{
    collect_code_files, is_supported_path, matches_path_filters, supported_extensions,
    WalkerOptions,
};

#[derive(Debug, Parser)]
#[command(name = "bonsai")]
#[command(version)]
#[command(about = "Shrink repository source context into token-efficient XML or JSON")]
struct Cli {
    #[arg(default_value = ".")]
    path: PathBuf,

    #[arg(long, default_value_t = 4000)]
    max_tokens: usize,

    #[arg(
        long,
        default_value_t = TokenizerKind::default(),
        value_name = "TOKENIZER",
        help = "Tokenizer family or model alias: o200k_base, cl100k_base, p50k_base, p50k_edit, r50k_base"
    )]
    tokenizer: TokenizerKind,

    #[arg(
        long,
        default_value_t = 1_048_576,
        help = "Skip files larger than this many bytes; 0 disables the cap"
    )]
    max_file_bytes: u64,

    #[arg(long, default_value_t = 2)]
    level: u8,

    #[arg(long, value_enum, default_value_t = OutputDestination::File)]
    output: OutputDestination,

    #[arg(long, default_value = "bonsai.xml")]
    output_file: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Xml)]
    format: OutputFormat,

    #[arg(long)]
    project_map_only: bool,

    #[arg(long, help = "Include stable content hashes in project map entries")]
    file_hashes: bool,

    #[arg(long)]
    no_content: bool,

    #[arg(
        long,
        help = "Print selected files and estimated tokens without writing output"
    )]
    dry_run: bool,

    #[arg(long, value_enum, default_value_t = SortMode::Path)]
    sort: SortMode,

    #[arg(long)]
    directory_summaries: bool,

    #[arg(long)]
    fail_over_budget: bool,

    #[arg(
        long,
        help = "Only include files added or changed since the last cached local run"
    )]
    incremental: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Only include files added or changed compared with a base directory or cache file"
    )]
    incremental_base: Option<PathBuf>,

    #[arg(
        long,
        value_name = "GIT_REF",
        help = "Only include files changed by git diff against this ref"
    )]
    changed_since: Option<String>,

    #[arg(
        long,
        help = "Print added, changed, unchanged, skipped, and deleted counts"
    )]
    incremental_summary: bool,

    #[arg(long, value_name = "GLOB")]
    include: Vec<String>,

    #[arg(long, value_name = "GLOB")]
    exclude: Vec<String>,

    #[arg(long = "no-respect-gitignore", action = ArgAction::SetFalse, default_value_t = true)]
    respect_gitignore: bool,

    #[arg(long)]
    print_files: bool,

    #[arg(long)]
    fail_on_empty: bool,

    #[arg(long, help = "Suppress normal stdout output for scripts")]
    quiet: bool,

    #[arg(long)]
    stats: bool,

    #[arg(long)]
    detailed_stats: bool,

    #[arg(long)]
    summary: bool,

    #[arg(long)]
    prompt: bool,

    #[arg(long, value_name = "TEXT")]
    ask_template: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputDestination {
    Clipboard,
    File,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Xml,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SortMode {
    Path,
    Tokens,
    Priority,
}

fn main() -> Result<()> {
    if handle_init_agent_command()? {
        return Ok(());
    }
    if handle_cache_command()? {
        return Ok(());
    }
    if handle_doctor_command()? {
        return Ok(());
    }

    let cli = Cli::parse();
    validate_delta_options(&cli)?;
    let root = fs::canonicalize(&cli.path)
        .with_context(|| format!("cannot resolve target path {}", cli.path.display()))?;
    let requested_level = CompressionLevel::try_from(cli.level)?;
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
            format_context(&full_files, &metadata, &cli, &deleted_files),
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
    let optimized = optimize_budget(files, content_budget, &token_counter)?;
    let (mut optimized, context, output_tokens) =
        fit_formatted_context(optimized, &metadata, &cli, &token_counter, &deleted_files)?;
    sort_files(&mut optimized, cli.sort);
    let run_stats = RunStats::new(
        &cli,
        requested_level,
        optimized.len(),
        raw_context.as_deref(),
        output_tokens,
        &token_counter,
    )?;

    if output_tokens > cli.max_tokens {
        if !cli.quiet {
            eprintln!(
                "warning: output is {output_tokens} tokens, above --max-tokens {} after all files reached tree map",
                cli.max_tokens
            );
        }
        if cli.fail_over_budget {
            bail!(
                "output is {output_tokens} tokens, above --max-tokens {}",
                cli.max_tokens
            );
        }
    }

    if cli.dry_run {
        if !cli.quiet {
            print_dry_run(&optimized, &deleted_files, output_tokens, cli.max_tokens);
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
                fs::write(&cli.output_file, context)
                    .with_context(|| format!("cannot write {}", cli.output_file.display()))?;
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

fn handle_init_agent_command() -> Result<bool> {
    let args = env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) != Some("init-agent") {
        return Ok(false);
    }

    let mut target = PathBuf::from(".");
    let mut force = false;
    let mut saw_path = false;

    for arg in args.iter().skip(2) {
        match arg.as_str() {
            "--force" | "-f" => force = true,
            "--help" | "-h" => {
                println!("Usage: bonsai init-agent [PATH] [--force]");
                return Ok(true);
            }
            value if value.starts_with('-') => bail!("unknown init-agent option {value}"),
            value => {
                if saw_path {
                    bail!("init-agent accepts only one path");
                }
                target = PathBuf::from(value);
                saw_path = true;
            }
        }
    }

    init_agent_files(&target, force)?;
    Ok(true)
}

fn handle_cache_command() -> Result<bool> {
    let args = env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) != Some("cache") {
        return Ok(false);
    }

    match args.get(2).map(String::as_str) {
        Some("clear") => {
            let target = cache_command_target(&args[3..])?;
            clear_cache(&target)?;
            Ok(true)
        }
        Some("--help") | Some("-h") | None => {
            println!("Usage: bonsai cache clear [PATH]");
            Ok(true)
        }
        Some(command) => bail!("unknown cache command {command}"),
    }
}

fn handle_doctor_command() -> Result<bool> {
    let args = env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) != Some("doctor") {
        return Ok(false);
    }

    let options = doctor_options(&args[2..])?;
    print_doctor(&options.target, options.tokenizer)?;
    Ok(true)
}

struct DoctorOptions {
    target: PathBuf,
    tokenizer: TokenizerKind,
}

fn doctor_options(args: &[String]) -> Result<DoctorOptions> {
    let mut target = PathBuf::from(".");
    let mut tokenizer = TokenizerKind::default();
    let mut saw_path = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => {
                println!("Usage: bonsai doctor [PATH] [--tokenizer TOKENIZER]");
                std::process::exit(0);
            }
            "--tokenizer" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    bail!("--tokenizer requires a value");
                };
                tokenizer = value.parse::<TokenizerKind>().map_err(anyhow::Error::msg)?;
            }
            value if value.starts_with('-') => bail!("unknown doctor option {value}"),
            value => {
                if saw_path {
                    bail!("doctor accepts only one path");
                }
                target = PathBuf::from(value);
                saw_path = true;
            }
        }
        index += 1;
    }

    Ok(DoctorOptions { target, tokenizer })
}

fn print_doctor(target: &Path, tokenizer: TokenizerKind) -> Result<()> {
    let root = fs::canonicalize(target)
        .with_context(|| format!("cannot resolve doctor target {}", target.display()))?;
    let binary_path = env::current_exe().context("cannot resolve current executable")?;
    let cache_path = cache_path_for_root(&root);
    let tokenizer_status = TokenCounter::new(tokenizer)
        .map(|_| "ok".to_owned())
        .unwrap_or_else(|error| format!("error: {error:#}"));

    println!("bonsai doctor:");
    println!("  binary: {}", binary_path.display());
    println!("  version: {}", env!("CARGO_PKG_VERSION"));
    println!("  repo_root: {}", root.display());
    println!("  cache_path: {}", cache_path.display());
    println!("  tokenizer: {} ({tokenizer_status})", tokenizer.as_str());
    println!("  parsers:");

    for extension in supported_extensions() {
        let support = parser_support_for_extension(extension);
        let mode = match support.mode {
            ParserMode::TreeSitter => "tree-sitter",
            ParserMode::Compact => "compact",
        };
        let status = if support.available { "ok" } else { "missing" };
        println!("    .{}: {mode} ({status})", support.extension);
    }

    Ok(())
}

fn cache_command_target(args: &[String]) -> Result<PathBuf> {
    let mut target = PathBuf::from(".");
    let mut saw_path = false;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Usage: bonsai cache clear [PATH]");
                std::process::exit(0);
            }
            value if value.starts_with('-') => bail!("unknown cache clear option {value}"),
            value => {
                if saw_path {
                    bail!("cache clear accepts only one path");
                }
                target = PathBuf::from(value);
                saw_path = true;
            }
        }
    }

    Ok(target)
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

fn init_agent_files(target: &Path, force: bool) -> Result<()> {
    fs::create_dir_all(target)
        .with_context(|| format!("cannot create agent target {}", target.display()))?;

    let agent_path = target.join("AGENTS.md");
    let claude_path = target.join("CLAUDE.md");

    if !force {
        let existing = [agent_path.as_path(), claude_path.as_path()]
            .into_iter()
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

    write_agent_file(&agent_path, AGENTS_TEMPLATE)?;
    write_agent_file(&claude_path, CLAUDE_TEMPLATE)?;
    println!("wrote AGENTS.md and CLAUDE.md in {}", target.display());
    Ok(())
}

fn write_agent_file(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))
}

const AGENTS_TEMPLATE: &str = r#"# AGENTS.md

For repo-wide analysis, first run Bonsai. This is required for full project summaries, architecture review, onboarding, broad bug hunting, and any question that needs many files. Do not answer from ordinary file browsing until this command succeeds:

```sh
bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
```

Then inspect `/tmp/bonsai.xml` before answering.

Expected behavior example:

```text
User asks: summarize this whole project
You run: bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
You inspect: /tmp/bonsai.xml
Then answer from that context.
```
"#;

const CLAUDE_TEMPLATE: &str = r#"# CLAUDE.md

For repo-wide analysis, first run Bonsai. This is required for full project summaries, architecture review, onboarding, broad bug hunting, and any question that needs many files. Do not answer from ordinary file browsing until this command succeeds:

```sh
bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
```

Then inspect `/tmp/bonsai.xml` before answering.

Expected behavior example:

```text
User asks: summarize this whole project
Claude runs: bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
Claude inspects: /tmp/bonsai.xml
Claude answers from that context.
```
"#;

fn max_file_bytes(cli: &Cli) -> Option<u64> {
    if cli.max_file_bytes == 0 {
        None
    } else {
        Some(cli.max_file_bytes)
    }
}

fn validate_delta_options(cli: &Cli) -> Result<()> {
    if cli.changed_since.is_some() && (cli.incremental || cli.incremental_base.is_some()) {
        bail!("--changed-since cannot be combined with --incremental or --incremental-base");
    }
    Ok(())
}

fn cache_metadata(cli: &Cli) -> CacheMetadata {
    CacheMetadata {
        include: cli.include.clone(),
        exclude: cli.exclude.clone(),
        respect_gitignore: cli.respect_gitignore,
        max_file_bytes: max_file_bytes(cli),
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

    let output = Command::new("git")
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

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git diff against {git_ref} failed: {}", stderr.trim());
    }

    parse_git_changes(&output.stdout, root, cli)
}

fn parse_git_changes(bytes: &[u8], root: &Path, cli: &Cli) -> Result<Option<GitChanges>> {
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
    Ok(Some(changes))
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
    selected_level: CompressionLevel,
    max_tokens: usize,
    tokenizer: TokenizerKind,
    raw_tokens: Option<usize>,
    shrunk_tokens: usize,
    tokens_saved: Option<usize>,
    saving_percent: Option<f64>,
}

impl RunStats {
    fn new(
        cli: &Cli,
        selected_level: CompressionLevel,
        files_scanned: usize,
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
            selected_level,
            max_tokens: cli.max_tokens,
            tokenizer: counter.tokenizer(),
            raw_tokens,
            shrunk_tokens,
            tokens_saved,
            saving_percent,
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
) -> String {
    let options = format_options(files, cli, deleted_files);
    match cli.format {
        OutputFormat::Json => format_repository_context_json(files, metadata, &options),
        OutputFormat::Xml => format_repository_context_xml(files, metadata, &options),
    }
}

fn format_options(files: &[ProcessedFile], cli: &Cli, deleted_files: &[String]) -> FormatOptions {
    FormatOptions {
        project_map_only: cli.project_map_only,
        include_file_hashes: cli.file_hashes,
        include_files: !cli.project_map_only && !cli.no_content,
        include_content: !cli.project_map_only && !cli.no_content,
        deleted_files: if cli.project_map_only {
            Vec::new()
        } else {
            deleted_files.to_vec()
        },
        directory_summaries: if cli.directory_summaries && !cli.project_map_only {
            build_directory_summaries(files)
        } else {
            Vec::new()
        },
    }
}

fn fit_formatted_context(
    mut files: Vec<ProcessedFile>,
    metadata: &RepositoryMetadata,
    cli: &Cli,
    counter: &TokenCounter,
    deleted_files: &[String],
) -> Result<(Vec<ProcessedFile>, String, usize)> {
    loop {
        sort_files(&mut files, cli.sort);
        let context = maybe_wrap_prompt(format_context(&files, metadata, cli, deleted_files), cli);
        let output_tokens = count_text_tokens(&context, counter);

        if output_tokens <= cli.max_tokens || !downgrade_largest_file(&mut files, counter) {
            return Ok((files, context, output_tokens));
        }
    }
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
        format_context(&overhead_files, metadata, cli, deleted_files),
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
            file_priority_score(right)
                .cmp(&file_priority_score(left))
                .then(left.path.cmp(&right.path))
        }),
    }
}

fn stable_content_hash(content: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
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
    println!("  selected_level: {}", stats.selected_level.as_u8());
    println!("  tokenizer: {}", stats.tokenizer.as_str());
    println!(
        "  output_tokens: {} / {}",
        stats.shrunk_tokens, stats.max_tokens
    );
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
) {
    println!("dry_run:");
    println!("  files: {}", files.len());
    println!("  deleted: {}", deleted_files.len());
    println!("  estimated_tokens: {estimated_tokens}");
    println!("  max_tokens: {max_tokens}");
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

        let (_files, context, output_tokens) =
            fit_formatted_context(files, &metadata, &cli, &counter, &[]).unwrap();

        assert_eq!(output_tokens, count_text_tokens(&context, &counter));
    }

    fn test_cli(format: OutputFormat) -> Cli {
        Cli {
            path: PathBuf::from("."),
            max_tokens: 12000,
            tokenizer: TokenizerKind::default(),
            max_file_bytes: 1_048_576,
            level: 2,
            output: OutputDestination::File,
            output_file: PathBuf::from("bonsai.xml"),
            format,
            project_map_only: false,
            file_hashes: false,
            no_content: false,
            dry_run: false,
            sort: SortMode::Path,
            directory_summaries: false,
            fail_over_budget: false,
            incremental: false,
            incremental_base: None,
            changed_since: None,
            incremental_summary: false,
            include: Vec::new(),
            exclude: Vec::new(),
            respect_gitignore: true,
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
