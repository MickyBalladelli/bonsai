mod budget;
mod formatter;
mod parser;
mod walker;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser, ValueEnum};

use budget::{count_text_tokens, optimize_budget, ProcessedFile};
use formatter::{
    format_repository_context_json, format_repository_context_xml, RepositoryMetadata,
};
use parser::{compress_file, CompressionLevel};
use walker::{collect_code_files, supported_extensions, WalkerOptions};

#[derive(Debug, Parser)]
#[command(name = "bonsai")]
#[command(version)]
#[command(about = "Shrink repository source context into token-efficient XML or JSON")]
struct Cli {
    #[arg(default_value = ".")]
    path: PathBuf,

    #[arg(long, default_value_t = 4000)]
    max_tokens: usize,

    #[arg(long, default_value_t = 2)]
    level: u8,

    #[arg(long, value_enum, default_value_t = OutputDestination::File)]
    output: OutputDestination,

    #[arg(long, default_value = "bonsai.xml")]
    output_file: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Xml)]
    format: OutputFormat,

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

    #[arg(long)]
    stats: bool,

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = fs::canonicalize(&cli.path)
        .with_context(|| format!("cannot resolve target path {}", cli.path.display()))?;
    let requested_level = CompressionLevel::try_from(cli.level)?;

    let paths = collect_code_files(
        &root,
        &WalkerOptions {
            include: cli.include.clone(),
            exclude: cli.exclude.clone(),
            respect_gitignore: cli.respect_gitignore,
        },
    )?;
    if paths.is_empty() {
        handle_empty_selection(&cli, &root)?;
    }

    if cli.print_files {
        print_selected_files(&root, &paths);
    }

    let mut files = Vec::with_capacity(paths.len());

    for path in paths {
        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let variants = compress_file(&path, requested_level)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        files.push(ProcessedFile::new(relative_path, requested_level, variants));
    }

    let full_files = full_context_files(&files)?;
    let optimized = optimize_budget(files, cli.max_tokens)?;
    let metadata = RepositoryMetadata {
        generated_at: generated_at_unix()?,
        repo_root: root.display().to_string(),
        max_tokens: cli.max_tokens,
        compression_level: requested_level.as_u8(),
        file_count: optimized.len(),
    };
    let raw_context = maybe_wrap_prompt(format_context(&full_files, &metadata, cli.format), &cli);
    let context = maybe_wrap_prompt(format_context(&optimized, &metadata, cli.format), &cli);
    let run_stats = RunStats::new(
        &cli,
        requested_level,
        optimized.len(),
        &raw_context,
        &context,
    )?;

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

    if cli.summary {
        print_summary(&run_stats);
    }

    if cli.stats {
        print_stats(&run_stats);
    }

    Ok(())
}

#[derive(Debug)]
struct RunStats {
    output_target: String,
    files_scanned: usize,
    selected_level: CompressionLevel,
    max_tokens: usize,
    raw_tokens: usize,
    shrunk_tokens: usize,
    tokens_saved: usize,
    saving_percent: f64,
}

impl RunStats {
    fn new(
        cli: &Cli,
        selected_level: CompressionLevel,
        files_scanned: usize,
        raw_xml: &str,
        shrunk_xml: &str,
    ) -> Result<Self> {
        let raw_tokens = count_text_tokens(raw_xml)?;
        let shrunk_tokens = count_text_tokens(shrunk_xml)?;
        let tokens_saved = raw_tokens.saturating_sub(shrunk_tokens);
        let saving_percent = if raw_tokens == 0 {
            0.0
        } else {
            tokens_saved as f64 / raw_tokens as f64 * 100.0
        };

        Ok(Self {
            output_target: output_target(cli),
            files_scanned,
            selected_level,
            max_tokens: cli.max_tokens,
            raw_tokens,
            shrunk_tokens,
            tokens_saved,
            saving_percent,
        })
    }
}

fn full_context_files(files: &[ProcessedFile]) -> Result<Vec<ProcessedFile>> {
    files
        .iter()
        .cloned()
        .map(|mut file| {
            file.level = CompressionLevel::Full;
            file.token_count = count_text_tokens(file.content())?;
            Ok(file)
        })
        .collect()
}

fn format_context(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
    output_format: OutputFormat,
) -> String {
    match output_format {
        OutputFormat::Json => format_repository_context_json(files, metadata),
        OutputFormat::Xml => format_repository_context_xml(files, metadata),
    }
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

    eprintln!("warning: {message}");
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
    println!(
        "  output_tokens: {} / {}",
        stats.shrunk_tokens, stats.max_tokens
    );
}

fn print_stats(stats: &RunStats) {
    println!("stats:");
    println!("  raw_tokens: {}", stats.raw_tokens);
    println!("  shrunk_tokens: {}", stats.shrunk_tokens);
    println!("  tokens_saved: {}", stats.tokens_saved);
    println!("  saving_percent: {:.2}", stats.saving_percent);
    println!("  files_scanned: {}", stats.files_scanned);
}
