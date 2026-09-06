use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};

pub const TARGET_EXTENSIONS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "py", "rs", "go", "java", "cs", "swift", "kt", "c", "h", "cpp",
    "hpp", "m", "mm", "vue", "svelte", "astro", "html", "md", "json", "yaml", "yml", "toml",
];

#[derive(Debug, Clone)]
pub struct WalkerOptions {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub respect_gitignore: bool,
    pub max_file_bytes: Option<u64>,
    pub exclude_generated: bool,
    /// Absolute path of Bonsai's configured output file. The output file and
    /// its numbered chunks (`base-2.ext`, ...) are never source input, even
    /// when they use a scanned extension (for example `bonsai.json`) and are
    /// not gitignored. `None` disables the exclusion (for example clipboard
    /// runs, which write no file).
    pub output_file: Option<PathBuf>,
}

pub fn collect_code_files(root: &Path, options: &WalkerOptions) -> Result<Vec<PathBuf>> {
    let files = Mutex::new(Vec::new());
    let filters = Arc::new(PathFilters::new(&options.include, &options.exclude)?);
    let max_file_bytes = options.max_file_bytes;
    let exclude_generated = options.exclude_generated;
    let output_file = options.output_file.clone();
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
        let output_file = &output_file;
        Box::new(move |result| {
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => return WalkState::Continue,
            };

            if let Some(output) = output_file {
                if is_bonsai_output_artifact(root, entry.path(), output) {
                    return WalkState::Continue;
                }
            }

            if is_target_file(&entry)
                && fits_size_limit(&entry, max_file_bytes)
                && filters.matches(root, entry.path())
                && (!exclude_generated
                    || filters.explicitly_includes(root, entry.path())
                    || !is_generated_like(root, entry.path()))
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

/// True when `path` is Bonsai's configured output file or one of its numbered
/// chunks (`base-2.ext`, `base-3.ext`, ...) in the same directory. Both paths
/// are compared relative to `root`; an output file outside `root` never
/// matches. Only the output's own stem and extension match, so unrelated
/// files such as `other.json` or `bonsai-notes.md` are kept.
pub fn is_bonsai_output_artifact(root: &Path, path: &Path, output: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let Ok(output_relative) = output.strip_prefix(root) else {
        return false;
    };
    if relative == output_relative {
        return true;
    }
    if relative.parent() != output_relative.parent() {
        return false;
    }
    let (Some(output_name), Some(candidate_name)) = (
        output_relative.file_name().and_then(|name| name.to_str()),
        relative.file_name().and_then(|name| name.to_str()),
    ) else {
        return false;
    };
    is_numbered_chunk_name(output_name, candidate_name)
}

/// Absolute path of numbered chunk `index` (0-based) for `output`.
/// Index 0 is the output itself; index N >= 1 is `base-(N+1).ext`,
/// mirroring `chunkOutputPath` in the VS Code extension.
pub fn numbered_chunk_path(output: &Path, index: usize) -> PathBuf {
    if index == 0 {
        return output.to_path_buf();
    }
    let file_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let (stem, extension) = split_file_name(file_name);
    let chunk_name = match extension {
        Some(extension) => format!("{}-{}.{}", stem, index + 1, extension),
        None => format!("{}-{}", stem, index + 1),
    };
    match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(chunk_name),
        _ => PathBuf::from(chunk_name),
    }
}

/// Delete numbered chunks of `output` at positions `>= keep_chunks`.
///
/// Only files matching the output's own stem and extension
/// (`base-2.ext`, `base-3.ext`, ...) are removed; unrelated files such as
/// `other.json` or `bonsai-notes.md` are never touched. The base output
/// itself is never removed. Missing directories and unreadable entries are
/// ignored, and per-file removal errors are skipped so cleanup never fails
/// generation. Returns the paths that were removed.
pub fn retire_stale_chunks(output: &Path, keep_chunks: usize) -> Vec<PathBuf> {
    let directory: &Path = match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let output_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let candidate_path = entry.path();
        let Some(candidate_name) = candidate_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if candidate_name == output_name {
            continue;
        }
        let Some(chunk_number) = chunk_number_for(output_name, candidate_name) else {
            continue;
        };
        if (chunk_number as usize) <= keep_chunks {
            continue;
        }
        if std::fs::remove_file(&candidate_path).is_ok() {
            removed.push(candidate_path);
        }
    }
    removed.sort();
    removed
}

/// True when `candidate` is `output` with `-N` (N >= 2) inserted before the
/// extension, mirroring the extension's chunk naming (`base-2.ext`).
fn is_numbered_chunk_name(output_name: &str, candidate: &str) -> bool {
    chunk_number_for(output_name, candidate).is_some()
}

/// Chunk number N when `candidate` is `output` with `-N` (N >= 2) inserted
/// before the extension; `None` for unrelated files.
fn chunk_number_for(output_name: &str, candidate: &str) -> Option<u32> {
    let (output_stem, output_extension) = split_file_name(output_name);
    let (candidate_stem, candidate_extension) = split_file_name(candidate);
    if output_extension != candidate_extension {
        return None;
    }
    let suffix = candidate_stem.strip_prefix(output_stem)?;
    let digits = suffix.strip_prefix('-')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let chunk: u32 = digits.parse().ok()?;
    (chunk >= 2).then_some(chunk)
}

fn split_file_name(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, extension)) => (stem, Some(extension)),
        None => (name, None),
    }
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

    fn explicitly_includes(&self, root: &Path, path: &Path) -> bool {
        let relative_path = path.strip_prefix(root).unwrap_or(path);
        self.include
            .as_ref()
            .is_some_and(|include| include.is_match(relative_path))
    }
}

pub fn is_generated_like(root: &Path, path: &Path) -> bool {
    let relative_path = path.strip_prefix(root).unwrap_or(path);
    let components = relative_path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();

    if components.iter().any(|component| {
        matches!(
            component.as_str(),
            "node_modules"
                | "vendor"
                | "vendors"
                | "third_party"
                | "third-party"
                | "external"
                | "generated"
                | "__generated__"
                | "dist"
                | "build"
                | "out"
                | "target"
        )
    }) {
        return true;
    }

    let Some(file_name) = components.last() else {
        return false;
    };

    is_lockfile_like(file_name) || is_minified_like(file_name) || is_generated_file_name(file_name)
}

fn is_lockfile_like(file_name: &str) -> bool {
    matches!(
        file_name,
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

fn is_minified_like(file_name: &str) -> bool {
    file_name.contains(".min.")
}

fn is_generated_file_name(file_name: &str) -> bool {
    file_name.contains("generated")
        || file_name.contains(".gen.")
        || file_name.contains("_gen.")
        || file_name.ends_with(".pb.go")
        || file_name.ends_with(".pb.rs")
        || file_name.ends_with(".pb.swift")
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
        write_file(&root, "main.c", "");
        write_file(&root, "main.h", "");
        write_file(&root, "main.cpp", "");
        write_file(&root, "main.hpp", "");
        write_file(&root, "View.m", "");
        write_file(&root, "View.mm", "");
        write_file(&root, "App.vue", "");
        write_file(&root, "App.svelte", "");
        write_file(&root, "Page.astro", "");
        write_file(&root, "index.html", "");
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
        assert!(names.contains(&"main.c".to_owned()));
        assert!(names.contains(&"main.h".to_owned()));
        assert!(names.contains(&"main.cpp".to_owned()));
        assert!(names.contains(&"main.hpp".to_owned()));
        assert!(names.contains(&"View.m".to_owned()));
        assert!(names.contains(&"View.mm".to_owned()));
        assert!(names.contains(&"App.vue".to_owned()));
        assert!(names.contains(&"App.svelte".to_owned()));
        assert!(names.contains(&"Page.astro".to_owned()));
        assert!(names.contains(&"index.html".to_owned()));
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
                exclude_generated: false,
                output_file: None,
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
                exclude_generated: false,
                output_file: None,
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
                exclude_generated: false,
                output_file: None,
            },
        )
        .unwrap();
        let names = relative_names(&root, files);

        assert_eq!(names, vec!["small.rs"]);
    }

    #[test]
    fn can_exclude_generated_like_files() {
        let root = temp_dir();
        fs::create_dir_all(root.join("src/generated")).unwrap();
        fs::create_dir_all(root.join("vendor")).unwrap();
        fs::create_dir_all(root.join("dist")).unwrap();
        write_file(&root, "src/app.rs", "");
        write_file(&root, "src/generated/types.ts", "");
        write_file(&root, "src/client.min.js", "");
        write_file(&root, "src/schema.pb.go", "");
        write_file(&root, "vendor/lib.rs", "");
        write_file(&root, "dist/app.js", "");
        write_file(&root, "package-lock.json", "{}");

        let files = collect_code_files(
            &root,
            &WalkerOptions {
                include: Vec::new(),
                exclude: Vec::new(),
                respect_gitignore: true,
                max_file_bytes: Some(1_048_576),
                exclude_generated: true,
                output_file: None,
            },
        )
        .unwrap();
        let names = relative_names(&root, files);

        assert_eq!(names, vec!["src/app.rs"]);
    }

    #[test]
    fn include_overrides_generated_exclusion() {
        let root = temp_dir();
        fs::create_dir_all(root.join("src/generated")).unwrap();
        write_file(&root, "src/app.rs", "");
        write_file(&root, "src/generated/types.ts", "");

        let files = collect_code_files(
            &root,
            &WalkerOptions {
                include: vec!["src/generated/**".to_owned()],
                exclude: Vec::new(),
                respect_gitignore: true,
                max_file_bytes: Some(1_048_576),
                exclude_generated: true,
                output_file: None,
            },
        )
        .unwrap();
        let names = relative_names(&root, files);

        assert_eq!(names, vec!["src/generated/types.ts"]);
    }

    #[test]
    fn excludes_configured_output_and_numbered_chunks() {
        let root = temp_dir();
        fs::create_dir_all(root.join("src")).unwrap();
        write_file(&root, "src/app.rs", "");
        write_file(&root, "bonsai.json", "{}");
        write_file(&root, "bonsai-2.json", "{}");
        write_file(&root, "bonsai-3.json", "{}");
        write_file(&root, "other.json", "{}");
        write_file(&root, "bonsai-notes.md", "");

        let output = root.join("bonsai.json");
        let files = collect_code_files(
            &root,
            &WalkerOptions {
                include: Vec::new(),
                exclude: Vec::new(),
                respect_gitignore: false,
                max_file_bytes: Some(1_048_576),
                exclude_generated: false,
                output_file: Some(output),
            },
        )
        .unwrap();
        let names = relative_names(&root, files);

        assert!(names.contains(&"src/app.rs".to_owned()));
        assert!(names.contains(&"other.json".to_owned()));
        assert!(names.contains(&"bonsai-notes.md".to_owned()));
        assert!(!names.contains(&"bonsai.json".to_owned()));
        assert!(!names.contains(&"bonsai-2.json".to_owned()));
        assert!(!names.contains(&"bonsai-3.json".to_owned()));
    }

    #[test]
    fn output_exclusion_cannot_be_overridden_by_include() {
        let root = temp_dir();
        write_file(&root, "bonsai.json", "{}");

        let output = root.join("bonsai.json");
        let files = collect_code_files(
            &root,
            &WalkerOptions {
                include: vec!["bonsai.json".to_owned()],
                exclude: Vec::new(),
                respect_gitignore: false,
                max_file_bytes: Some(1_048_576),
                exclude_generated: false,
                output_file: Some(output),
            },
        )
        .unwrap();

        assert!(relative_names(&root, files).is_empty());
    }

    #[test]
    fn retire_stale_chunks_keeps_active_chunks_and_unrelated_files() {
        let root = temp_dir();
        let output = root.join("bonsai.json");
        write_file(&root, "bonsai.json", "base");
        write_file(&root, "bonsai-2.json", "stale");
        write_file(&root, "bonsai-3.json", "stale");
        write_file(&root, "other.json", "keep");
        write_file(&root, "bonsai-notes.md", "keep");

        let removed = retire_stale_chunks(&output, 1);

        assert_eq!(
            removed,
            vec![root.join("bonsai-2.json"), root.join("bonsai-3.json")]
        );
        assert!(output.is_file());
        assert!(!root.join("bonsai-2.json").exists());
        assert!(!root.join("bonsai-3.json").exists());
        assert!(root.join("other.json").is_file());
        assert!(root.join("bonsai-notes.md").is_file());
    }

    #[test]
    fn numbered_chunk_paths_mirror_extension_naming() {
        let output = PathBuf::from("/tmp/out/bonsai.xml");
        assert_eq!(numbered_chunk_path(&output, 0), output);
        assert_eq!(
            numbered_chunk_path(&output, 1),
            PathBuf::from("/tmp/out/bonsai-2.xml")
        );
        assert_eq!(
            numbered_chunk_path(&output, 2),
            PathBuf::from("/tmp/out/bonsai-3.xml")
        );
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
            exclude_generated: false,
            output_file: None,
        }
    }
}
