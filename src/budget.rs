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

        downgrade_file(&mut files[index], &encoder);
    }

    Ok(files)
}

pub fn downgrade_largest_file(files: &mut [ProcessedFile]) -> Result<bool> {
    let encoder = cl100k_base().context("cannot load cl100k tokenizer")?;
    let Some(index) = pick_downgrade_candidate(files) else {
        return Ok(false);
    };

    downgrade_file(&mut files[index], &encoder);
    Ok(true)
}

pub fn count_text_tokens(text: &str) -> Result<usize> {
    let encoder = cl100k_base().context("cannot load cl100k tokenizer")?;
    Ok(count_tokens(&encoder, text))
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

fn downgrade_file(file: &mut ProcessedFile, encoder: &CoreBPE) {
    file.level = match file.level {
        CompressionLevel::Full => CompressionLevel::Skeleton,
        CompressionLevel::Skeleton => CompressionLevel::TreeMap,
        CompressionLevel::TreeMap => CompressionLevel::TreeMap,
    };
    file.token_count = count_tokens(encoder, file.content());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downgrades_full_to_skeleton_when_over_budget() {
        let file = processed_file(
            "src/main.rs",
            CompressionLevel::Full,
            "fn main() { let value = \"this full body has enough repeated words to exceed budget\"; println!(\"{value}\"); }",
            "fn main() { ... }",
            "fn main()",
        );
        let max_tokens = count_text_tokens("fn main() { ... }").unwrap();

        let optimized = optimize_budget(vec![file], max_tokens).unwrap();

        assert_eq!(optimized[0].level, CompressionLevel::Skeleton);
        assert_eq!(optimized[0].content(), "fn main() { ... }");
    }

    #[test]
    fn downgrades_skeleton_to_tree_map_when_over_budget() {
        let file = processed_file(
            "src/main.rs",
            CompressionLevel::Skeleton,
            "fn main() { println!(\"full body\"); }",
            "fn main() { let one = 1; let two = 2; let three = 3; }",
            "fn main()",
        );
        let max_tokens = count_text_tokens("fn main()").unwrap();

        let optimized = optimize_budget(vec![file], max_tokens).unwrap();

        assert_eq!(optimized[0].level, CompressionLevel::TreeMap);
        assert_eq!(optimized[0].content(), "fn main()");
    }

    #[test]
    fn downgrade_order_is_stable_for_same_inputs() {
        let files = vec![
            processed_file(
                "src/a.rs",
                CompressionLevel::Full,
                "fn a() { let value = \"alpha alpha alpha alpha alpha alpha alpha\"; }",
                "fn a() { ... }",
                "fn a()",
            ),
            processed_file(
                "src/b.rs",
                CompressionLevel::Full,
                "fn b() { let value = \"beta beta beta beta beta beta beta\"; }",
                "fn b() { ... }",
                "fn b()",
            ),
        ];

        let first = optimize_budget(files.clone(), 1).unwrap();
        let second = optimize_budget(files, 1).unwrap();

        assert_eq!(levels(&first), levels(&second));
        assert_eq!(paths(&first), vec!["src/a.rs", "src/b.rs"]);
    }

    fn processed_file(
        path: &str,
        level: CompressionLevel,
        full: &str,
        skeleton: &str,
        tree_map: &str,
    ) -> ProcessedFile {
        ProcessedFile::new(
            path.to_owned(),
            level,
            FileVariants {
                full: Some(full.to_owned()),
                skeleton: skeleton.to_owned(),
                tree_map: tree_map.to_owned(),
            },
        )
    }

    fn levels(files: &[ProcessedFile]) -> Vec<CompressionLevel> {
        files.iter().map(|file| file.level).collect()
    }

    fn paths(files: &[ProcessedFile]) -> Vec<&str> {
        files.iter().map(|file| file.path.as_str()).collect()
    }
}
