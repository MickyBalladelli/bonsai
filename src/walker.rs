use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};

pub const TARGET_EXTENSIONS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "py", "rs", "go", "java", "cs", "swift", "kt", "md", "json", "yaml",
    "yml", "toml",
];

#[derive(Debug, Clone)]
pub struct WalkerOptions {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub respect_gitignore: bool,
    pub max_file_bytes: Option<u64>,
}

pub fn collect_code_files(root: &Path, options: &WalkerOptions) -> Result<Vec<PathBuf>> {
    let files = Mutex::new(Vec::new());
    let filters = Arc::new(PathFilters::new(&options.include, &options.exclude)?);
    let max_file_bytes = options.max_file_bytes;
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(options.respect_gitignore)
        .git_global(options.respect_gitignore)
        .git_exclude(options.respect_gitignore)
        .parents(options.respect_gitignore)
        .ignore(true)
        .add_custom_ignore_filename(".cursorignore")
        .threads(0);

    builder.build_parallel().run(|| {
        let files = &files;
        let filters = Arc::clone(&filters);
        Box::new(move |result| {
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => return WalkState::Continue,
            };

            if is_target_file(&entry)
                && fits_size_limit(&entry, max_file_bytes)
                && filters.matches(root, entry.path())
            {
                if let Some(path) = entry.path().to_str() {
                    if path.contains("/.git/") {
                        return WalkState::Continue;
                    }
                }

                if let Ok(mut guard) = files.lock() {
                    guard.push(entry.into_path());
                }
            }

            WalkState::Continue
        })
    });

    let mut files = files.into_inner().context("file walker lock poisoned")?;
    files.sort_unstable();
    Ok(files)
}

pub fn supported_extensions() -> &'static [&'static str] {
    TARGET_EXTENSIONS
}

pub fn is_supported_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| TARGET_EXTENSIONS.contains(&extension.as_str()))
}

pub fn matches_path_filters(
    root: &Path,
    path: &Path,
    include: &[String],
    exclude: &[String],
) -> Result<bool> {
    Ok(PathFilters::new(include, exclude)?.matches(root, path))
}

fn fits_size_limit(entry: &DirEntry, max_file_bytes: Option<u64>) -> bool {
    max_file_bytes
        .and_then(|limit| {
            entry
                .metadata()
                .ok()
                .map(|metadata| metadata.len() <= limit)
        })
        .unwrap_or(true)
}

fn is_target_file(entry: &DirEntry) -> bool {
    entry
        .file_type()
        .is_some_and(|file_type| file_type.is_file())
        && is_supported_path(entry.path())
}

struct PathFilters {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl PathFilters {
    fn new(include: &[String], exclude: &[String]) -> Result<Self> {
        Ok(Self {
            include: build_glob_set(include)?,
            exclude: build_glob_set(exclude)?,
        })
    }

    fn matches(&self, root: &Path, path: &Path) -> bool {
        let relative_path = path.strip_prefix(root).unwrap_or(path);

        if self
            .exclude
            .as_ref()
            .is_some_and(|exclude| exclude.is_match(relative_path))
        {
            return false;
        }

        self.include
            .as_ref()
            .map(|include| include.is_match(relative_path))
            .unwrap_or(true)
    }
}

fn build_glob_set(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).with_context(|| format!("invalid glob pattern {pattern}"))?);
    }

    builder
        .build()
        .map(Some)
        .context("cannot build glob filters")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn collects_supported_extensions() {
        let root = temp_dir();
        write_file(&root, "main.rs", "");
        write_file(&root, "server.go", "");
        write_file(&root, "App.java", "");
        write_file(&root, "Program.cs", "");
        write_file(&root, "View.swift", "");
        write_file(&root, "Service.kt", "");
        write_file(&root, "README.md", "");
        write_file(&root, "package.json", "{}");
        write_file(&root, "config.yaml", "");
        write_file(&root, "config.yml", "");
        write_file(&root, "Cargo.toml", "");
        write_file(&root, "image.png", "");

        let files = collect_code_files(&root, &default_options()).unwrap();
        let names = relative_names(&root, files);

        assert!(names.contains(&"main.rs".to_owned()));
        assert!(names.contains(&"server.go".to_owned()));
        assert!(names.contains(&"App.java".to_owned()));
        assert!(names.contains(&"Program.cs".to_owned()));
        assert!(names.contains(&"View.swift".to_owned()));
        assert!(names.contains(&"Service.kt".to_owned()));
        assert!(names.contains(&"README.md".to_owned()));
        assert!(names.contains(&"package.json".to_owned()));
        assert!(names.contains(&"config.yaml".to_owned()));
        assert!(names.contains(&"config.yml".to_owned()));
        assert!(names.contains(&"Cargo.toml".to_owned()));
        assert!(!names.contains(&"image.png".to_owned()));
    }

    #[test]
    fn respects_gitignore_and_cursorignore() {
        let root = temp_dir();
        fs::create_dir(root.join(".git")).unwrap();
        write_file(&root, ".gitignore", "ignored.rs\n");
        write_file(&root, ".cursorignore", "cursor_ignored.ts\n");
        write_file(&root, "visible.rs", "");
        write_file(&root, "ignored.rs", "");
        write_file(&root, "cursor_ignored.ts", "");

        let files = collect_code_files(&root, &default_options()).unwrap();
        let names = relative_names(&root, files);

        assert!(names.contains(&"visible.rs".to_owned()));
        assert!(!names.contains(&"ignored.rs".to_owned()));
        assert!(!names.contains(&"cursor_ignored.ts".to_owned()));
    }

    #[test]
    fn include_and_exclude_filter_selected_files() {
        let root = temp_dir();
        fs::create_dir(root.join("src")).unwrap();
        fs::create_dir(root.join("tests")).unwrap();
        write_file(&root, "src/main.rs", "");
        write_file(&root, "src/generated.rs", "");
        write_file(&root, "tests/main.rs", "");

        let files = collect_code_files(
            &root,
            &WalkerOptions {
                include: vec!["src/**".to_owned()],
                exclude: vec!["**/generated.rs".to_owned()],
                respect_gitignore: true,
                max_file_bytes: Some(1_048_576),
            },
        )
        .unwrap();
        let names = relative_names(&root, files);

        assert_eq!(names, vec!["src/main.rs"]);
    }

    #[test]
    fn can_disable_gitignore_filtering() {
        let root = temp_dir();
        fs::create_dir(root.join(".git")).unwrap();
        write_file(&root, ".gitignore", "ignored.rs\n");
        write_file(&root, "ignored.rs", "");

        let files = collect_code_files(
            &root,
            &WalkerOptions {
                include: Vec::new(),
                exclude: Vec::new(),
                respect_gitignore: false,
                max_file_bytes: Some(1_048_576),
            },
        )
        .unwrap();
        let names = relative_names(&root, files);

        assert!(names.contains(&"ignored.rs".to_owned()));
    }

    #[test]
    fn skips_files_over_size_limit() {
        let root = temp_dir();
        write_file(&root, "small.rs", "fn a() {}");
        write_file(&root, "large.rs", "fn large() {}");

        let files = collect_code_files(
            &root,
            &WalkerOptions {
                include: Vec::new(),
                exclude: Vec::new(),
                respect_gitignore: true,
                max_file_bytes: Some(12),
            },
        )
        .unwrap();
        let names = relative_names(&root, files);

        assert_eq!(names, vec!["small.rs"]);
    }

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("bonsai-walker-{unique}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_file(root: &Path, name: &str, contents: &str) {
        fs::write(root.join(name), contents).unwrap();
    }

    fn relative_names(root: &Path, files: Vec<PathBuf>) -> Vec<String> {
        files
            .into_iter()
            .map(|path| {
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    fn default_options() -> WalkerOptions {
        WalkerOptions {
            include: Vec::new(),
            exclude: Vec::new(),
            respect_gitignore: true,
            max_file_bytes: Some(1_048_576),
        }
    }
}
