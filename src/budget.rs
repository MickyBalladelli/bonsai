use std::fmt;
use std::str::FromStr;

use anyhow::{Context, Result};
use tiktoken_rs::{cl100k_base, o200k_base, p50k_base, p50k_edit, r50k_base, CoreBPE};

use crate::parser::{CompressionLevel, FileVariants};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerKind {
    O200kBase,
    Cl100kBase,
    P50kBase,
    P50kEdit,
    R50kBase,
}

impl Default for TokenizerKind {
    fn default() -> Self {
        Self::Cl100kBase
    }
}

impl TokenizerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::Cl100kBase => "cl100k_base",
            Self::P50kBase => "p50k_base",
            Self::P50kEdit => "p50k_edit",
            Self::R50kBase => "r50k_base",
        }
    }
}

impl fmt::Display for TokenizerKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TokenizerKind {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase().replace(['-', '.'], "_");
        let starts_with_any =
            |prefixes: &[&str]| prefixes.iter().any(|prefix| normalized.starts_with(prefix));

        if starts_with_any(&["gpt_4o", "gpt_4_1", "gpt_5", "o1", "o3", "o4"]) {
            return Ok(Self::O200kBase);
        }

        if starts_with_any(&["gpt_4", "gpt_3_5", "text_embedding_ada_002"]) {
            return Ok(Self::Cl100kBase);
        }

        match normalized.as_str() {
            "o200k" | "o200k_base" => Ok(Self::O200kBase),
            "cl100k" | "cl100k_base" | "openai" | "chatgpt" => Ok(Self::Cl100kBase),
            "p50k" | "p50k_base" | "codex" | "code" | "text_davinci_002"
            | "text_davinci_003" => Ok(Self::P50kBase),
            "p50k_edit" | "text_davinci_edit_001" | "code_davinci_edit_001" => {
                Ok(Self::P50kEdit)
            }
            "r50k" | "r50k_base" | "gpt2" | "gpt_2" | "davinci" => Ok(Self::R50kBase),
            _ => Err(format!(
                "unknown tokenizer {value}; use o200k_base, cl100k_base, p50k_base, p50k_edit, or r50k_base"
            )),
        }
    }
}

pub struct TokenCounter {
    tokenizer: TokenizerKind,
    encoder: CoreBPE,
}

impl TokenCounter {
    pub fn new(tokenizer: TokenizerKind) -> Result<Self> {
        let encoder = match tokenizer {
            TokenizerKind::O200kBase => o200k_base(),
            TokenizerKind::Cl100kBase => cl100k_base(),
            TokenizerKind::P50kBase => p50k_base(),
            TokenizerKind::P50kEdit => p50k_edit(),
            TokenizerKind::R50kBase => r50k_base(),
        }
        .with_context(|| format!("cannot load {} tokenizer", tokenizer.as_str()))?;

        Ok(Self { tokenizer, encoder })
    }

    pub fn tokenizer(&self) -> TokenizerKind {
        self.tokenizer
    }

    pub fn count(&self, text: &str) -> usize {
        count_tokens(&self.encoder, text)
    }
}

#[derive(Debug, Clone)]
pub struct ProcessedFile {
    pub path: String,
    pub level: CompressionLevel,
    pub variants: FileVariants,
    pub token_count: usize,
    pub content_hash: Option<String>,
}

impl ProcessedFile {
    pub fn new(path: String, level: CompressionLevel, variants: FileVariants) -> Self {
        Self {
            path,
            level,
            variants,
            token_count: 0,
            content_hash: None,
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
    counter: &TokenCounter,
) -> Result<Vec<ProcessedFile>> {
    refresh_counts(&mut files, counter);

    while total_tokens(&files) > max_tokens {
        let Some(index) = pick_downgrade_candidate(&files) else {
            break;
        };

        downgrade_file(&mut files[index], counter);
    }

    Ok(files)
}

pub fn downgrade_largest_file(files: &mut [ProcessedFile], counter: &TokenCounter) -> bool {
    let Some(index) = pick_downgrade_candidate(files) else {
        return false;
    };

    downgrade_file(&mut files[index], counter);
    true
}

pub fn cap_file_tokens(
    files: &mut [ProcessedFile],
    max_file_tokens: usize,
    counter: &TokenCounter,
) {
    refresh_counts(files, counter);

    for file in files {
        while file.token_count > max_file_tokens && file.level != CompressionLevel::TreeMap {
            downgrade_file(file, counter);
        }

        if file.token_count > max_file_tokens {
            truncate_current_file(file, max_file_tokens, counter);
        }
    }
}

pub fn count_text_tokens(text: &str, counter: &TokenCounter) -> usize {
    counter.count(text)
}

fn refresh_counts(files: &mut [ProcessedFile], counter: &TokenCounter) {
    for file in files {
        file.token_count = counter.count(file.content());
    }
}

fn total_tokens(files: &[ProcessedFile]) -> usize {
    files.iter().map(|file| file.token_count).sum()
}

fn count_tokens(encoder: &CoreBPE, text: &str) -> usize {
    encoder.encode_ordinary(text).len()
}

fn downgrade_file(file: &mut ProcessedFile, counter: &TokenCounter) {
    file.level = match file.level {
        CompressionLevel::Full => CompressionLevel::Skeleton,
        CompressionLevel::Skeleton => CompressionLevel::TreeMap,
        CompressionLevel::TreeMap => CompressionLevel::TreeMap,
    };
    file.token_count = counter.count(file.content());
}

fn truncate_current_file(file: &mut ProcessedFile, max_tokens: usize, counter: &TokenCounter) {
    let truncated = truncate_text_tokens(file.content(), max_tokens, counter);
    match file.level {
        CompressionLevel::Full => file.variants.full = Some(truncated),
        CompressionLevel::Skeleton => file.variants.skeleton = truncated,
        CompressionLevel::TreeMap => file.variants.tree_map = truncated,
    }
    file.token_count = counter.count(file.content());
}

fn truncate_text_tokens(text: &str, max_tokens: usize, counter: &TokenCounter) -> String {
    if counter.count(text) <= max_tokens {
        return text.to_owned();
    }

    let chars = text.chars().collect::<Vec<_>>();
    let mut low = 0usize;
    let mut high = chars.len();

    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate = format!("{}...", chars[..mid].iter().collect::<String>().trim_end());
        if counter.count(&candidate) <= max_tokens {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    let truncated = chars[..low]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_owned();
    if truncated.is_empty() {
        "...".to_owned()
    } else {
        format!("{truncated}...")
    }
}

fn pick_downgrade_candidate(files: &[ProcessedFile]) -> Option<usize> {
    files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.level != CompressionLevel::TreeMap)
        .max_by(|(_, left), (_, right)| {
            downgrade_score(left)
                .cmp(&downgrade_score(right))
                .then(left.token_count.cmp(&right.token_count))
                .then(left.path.cmp(&right.path))
        })
        .map(|(index, _)| index)
}

fn downgrade_score(file: &ProcessedFile) -> i64 {
    leaf_score(file) as i64 + file.token_count as i64 - file_priority_score(file) as i64
}

fn leaf_score(file: &ProcessedFile) -> usize {
    file.path.matches('/').count() * 1000 + file.path.len()
}

pub fn file_priority_score(file: &ProcessedFile) -> usize {
    let path = file.path.as_str();
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");

    // Lockfiles and dependency noise downgrade first so implementation files
    // keep useful detail. Checked before the generic config-extension rule
    // because most lockfiles use json/yaml extensions.
    if is_lockfile_name(name) {
        return 0;
    }

    if matches!(
        name,
        "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
            | "README.md"
            | "AGENTS.md"
            | "CLAUDE.md"
            | "Dockerfile"
            | "Makefile"
    ) {
        return 5_000;
    }

    if matches!(
        name,
        "main.rs"
            | "main.ts"
            | "main.js"
            | "main.py"
            | "index.ts"
            | "index.js"
            | "app.ts"
            | "app.js"
            | "server.ts"
            | "server.js"
    ) {
        return 4_000;
    }

    if is_implementation_extension(extension) {
        return 3_500;
    }

    if path.starts_with(".github/workflows/") || path.contains("/.github/workflows/") {
        return 3_000;
    }

    if matches!(extension, "toml" | "json" | "yaml" | "yml" | "md") {
        return 2_000;
    }

    0
}

fn is_lockfile_name(file_name: &str) -> bool {
    matches!(
        file_name.to_ascii_lowercase().as_str(),
        "package-lock.json"
            | "npm-shrinkwrap.json"
            | "pnpm-lock.yaml"
            | "pnpm-lock.yml"
            | "yarn.lock"
            | "bun.lock"
            | "bun.lockb"
            | "cargo.lock"
            | "poetry.lock"
            | "pdm.lock"
            | "composer.lock"
            | "gemfile.lock"
            | "go.sum"
    )
}

fn is_implementation_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "js" | "jsx"
            | "ts"
            | "tsx"
            | "py"
            | "rs"
            | "go"
            | "java"
            | "cs"
            | "swift"
            | "kt"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "m"
            | "mm"
            | "vue"
            | "svelte"
            | "astro"
            | "html"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_counter() -> TokenCounter {
        TokenCounter::new(TokenizerKind::default()).unwrap()
    }

    #[test]
    fn downgrades_full_to_skeleton_when_over_budget() {
        let file = processed_file(
            "src/main.rs",
            CompressionLevel::Full,
            "fn main() { let value = \"this full body has enough repeated words to exceed budget\"; println!(\"{value}\"); }",
            "fn main() { ... }",
            "fn main()",
        );
        let counter = test_counter();
        let max_tokens = count_text_tokens("fn main() { ... }", &counter);

        let optimized = optimize_budget(vec![file], max_tokens, &counter).unwrap();

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
        let counter = test_counter();
        let max_tokens = count_text_tokens("fn main()", &counter);

        let optimized = optimize_budget(vec![file], max_tokens, &counter).unwrap();

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

        let counter = test_counter();
        let first = optimize_budget(files.clone(), 1, &counter).unwrap();
        let second = optimize_budget(files, 1, &counter).unwrap();

        assert_eq!(levels(&first), levels(&second));
        assert_eq!(paths(&first), vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn keeps_manifest_before_leaf_file_under_pressure() {
        let files = vec![
            processed_file(
                "Cargo.toml",
                CompressionLevel::Full,
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
                "[package]\nname = \"demo\"",
            ),
            processed_file(
                "src/deep/leaf.rs",
                CompressionLevel::Full,
                "fn leaf() { let value = \"alpha alpha alpha alpha alpha alpha alpha\"; }",
                "fn leaf() { ... }",
                "fn leaf()",
            ),
        ];

        let counter = test_counter();
        let max_tokens = count_text_tokens(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nfn leaf() { ... }",
            &counter,
        );

        let optimized = optimize_budget(files, max_tokens, &counter).unwrap();

        assert_eq!(optimized[0].level, CompressionLevel::Full);
        assert_ne!(optimized[1].level, CompressionLevel::Full);
    }

    #[test]
    fn downgrades_lockfile_before_implementation_file() {
        let files = vec![
            processed_file(
                "copilot/bonsai-vscode/package-lock.json",
                CompressionLevel::Full,
                "{\"name\": \"demo\", \"version\": \"0.1.0\", \"packages\": {\"a\": {\"version\": \"1.0.0\"}}}",
                "{\"name\": \"demo\", \"version\": \"0.1.0\"}",
                "{\"name\": \"demo\"}",
            ),
            processed_file(
                "copilot/bonsai-vscode/src/internalGenerator.ts",
                CompressionLevel::Full,
                "export function generate() { return run(); }",
                "export function generate() { ... }",
                "generate",
            ),
        ];

        let counter = test_counter();
        let max_tokens = count_text_tokens(
            "{\"name\": \"demo\"}export function generate() { ... }",
            &counter,
        );

        let optimized = optimize_budget(files, max_tokens, &counter).unwrap();
        let by_path = |path: &str| {
            optimized
                .iter()
                .find(|file| file.path == path)
                .unwrap()
                .level
        };

        assert_eq!(
            by_path("copilot/bonsai-vscode/src/internalGenerator.ts"),
            CompressionLevel::Skeleton
        );
        assert_eq!(
            by_path("copilot/bonsai-vscode/package-lock.json"),
            CompressionLevel::TreeMap
        );
    }

    #[test]
    fn ranks_implementation_above_config_above_lockfile() {
        let score = |path: &str| {
            file_priority_score(&processed_file(
                path,
                CompressionLevel::Full,
                "full",
                "skeleton",
                "tree",
            ))
        };

        assert!(score("src/app.rs") > score("config/settings.json"));
        assert!(score("src/main.rs") >= score("src/app.rs"));
        assert!(score("config/settings.json") > score("package-lock.json"));
        assert!(score("Cargo.toml") > score("src/app.rs"));
        assert_eq!(score("package-lock.json"), 0);
        assert_eq!(score("Cargo.lock"), 0);
        assert_eq!(score("yarn.lock"), 0);
    }

    #[test]
    fn caps_single_file_before_global_budget() {
        let mut files = vec![processed_file(
            "src/huge.rs",
            CompressionLevel::Skeleton,
            "fn huge() { println!(\"full\"); }",
            "fn huge() { let alpha = 1; let beta = 2; let gamma = 3; let delta = 4; }",
            "fn huge alpha beta gamma delta epsilon zeta eta theta iota kappa lambda",
        )];
        let counter = test_counter();
        let max_tokens = 5;

        cap_file_tokens(&mut files, max_tokens, &counter);

        assert_eq!(files[0].level, CompressionLevel::TreeMap);
        assert!(files[0].content().ends_with("..."));
        assert!(files[0].token_count <= max_tokens);
    }

    #[test]
    fn parses_tokenizer_family_aliases() {
        assert_eq!(
            "gpt-4o".parse::<TokenizerKind>().unwrap(),
            TokenizerKind::O200kBase
        );
        assert_eq!(
            "gpt-4o-mini".parse::<TokenizerKind>().unwrap(),
            TokenizerKind::O200kBase
        );
        assert_eq!(
            "gpt-3.5-turbo".parse::<TokenizerKind>().unwrap(),
            TokenizerKind::Cl100kBase
        );
        assert_eq!(
            "gpt-4-turbo".parse::<TokenizerKind>().unwrap(),
            TokenizerKind::Cl100kBase
        );
        assert_eq!(
            "gpt2".parse::<TokenizerKind>().unwrap(),
            TokenizerKind::R50kBase
        );
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
