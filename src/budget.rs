use anyhow::{Context, Result};
use tiktoken_rs::{cl100k_base, CoreBPE};

use crate::parser::{CompressionLevel, FileVariants};

#[derive(Debug, Clone)]
pub struct ProcessedFile {
    pub path: String,
    pub level: CompressionLevel,
    pub variants: FileVariants,
    pub token_count: usize,
}

impl ProcessedFile {
    pub fn new(path: String, level: CompressionLevel, variants: FileVariants) -> Self {
        Self {
            path,
            level,
            variants,
            token_count: 0,
        }
    }

    pub fn content(&self) -> &str {
        match self.level {
            CompressionLevel::Full => self
                .variants
                .full
                .as_deref()
                .unwrap_or(&self.variants.skeleton),
            CompressionLevel::Skeleton => &self.variants.skeleton,
            CompressionLevel::TreeMap => &self.variants.tree_map,
        }
    }
}

pub fn optimize_budget(
    mut files: Vec<ProcessedFile>,
    max_tokens: usize,
) -> Result<Vec<ProcessedFile>> {
    let encoder = cl100k_base().context("cannot load cl100k tokenizer")?;

    refresh_counts(&mut files, &encoder);

    while total_tokens(&files) > max_tokens {
        let Some(index) = pick_downgrade_candidate(&files) else {
            break;
        };

        files[index].level = match files[index].level {
            CompressionLevel::Full => CompressionLevel::Skeleton,
            CompressionLevel::Skeleton => CompressionLevel::TreeMap,
            CompressionLevel::TreeMap => CompressionLevel::TreeMap,
        };
        files[index].token_count = count_tokens(&encoder, files[index].content());
    }

    Ok(files)
}

fn refresh_counts(files: &mut [ProcessedFile], encoder: &CoreBPE) {
    for file in files {
        file.token_count = count_tokens(encoder, file.content());
    }
}

fn total_tokens(files: &[ProcessedFile]) -> usize {
    files.iter().map(|file| file.token_count).sum()
}

fn count_tokens(encoder: &CoreBPE, text: &str) -> usize {
    encoder.encode_ordinary(text).len()
}

fn pick_downgrade_candidate(files: &[ProcessedFile]) -> Option<usize> {
    files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.level != CompressionLevel::TreeMap)
        .max_by(|(_, left), (_, right)| {
            leaf_score(left)
                .cmp(&leaf_score(right))
                .then(left.token_count.cmp(&right.token_count))
                .then(left.path.cmp(&right.path))
        })
        .map(|(index, _)| index)
}

fn leaf_score(file: &ProcessedFile) -> usize {
    file.path.matches('/').count() * 1000 + file.path.len()
}
