mod budget;
mod formatter;
mod parser;
mod walker;

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

use budget::{optimize_budget, ProcessedFile};
use formatter::format_repository_context;
use parser::{compress_file, CompressionLevel};
use walker::collect_code_files;

#[derive(Debug, Parser)]
#[command(name = "contextshrink")]
#[command(about = "Shrink repository source context into token-efficient XML")]
struct Cli {
    #[arg(default_value = ".")]
    path: PathBuf,

    #[arg(long, default_value_t = 4000)]
    max_tokens: usize,

    #[arg(long, default_value_t = 2)]
    level: u8,

    #[arg(long, value_enum, default_value_t = OutputDestination::File)]
    output: OutputDestination,

    #[arg(long, default_value = "contextshrink.xml")]
    output_file: PathBuf,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputDestination {
    Clipboard,
    File,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = fs::canonicalize(&cli.path)
        .with_context(|| format!("cannot resolve target path {}", cli.path.display()))?;
    let requested_level = CompressionLevel::try_from(cli.level)?;

    let paths = collect_code_files(&root)?;
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

    let optimized = optimize_budget(files, cli.max_tokens)?;
    let xml = format_repository_context(&optimized);

    match cli.output {
        OutputDestination::Clipboard => {
            let mut clipboard = arboard::Clipboard::new().context("cannot access clipboard")?;
            clipboard.set_text(xml).context("cannot write clipboard")?;
        }
        OutputDestination::File => {
            fs::write(&cli.output_file, xml)
                .with_context(|| format!("cannot write {}", cli.output_file.display()))?;
        }
    }

    Ok(())
}
