use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use ignore::{DirEntry, WalkBuilder, WalkState};

const TARGET_EXTENSIONS: &[&str] = &["js", "jsx", "ts", "tsx", "py", "rs"];

pub fn collect_code_files(root: &Path) -> Result<Vec<PathBuf>> {
    let files = Mutex::new(Vec::new());
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .ignore(true)
        .add_custom_ignore_filename(".cursorignore")
        .threads(0);

    builder.build_parallel().run(|| {
        let files = &files;
        Box::new(move |result| {
            let entry = match result {
                Ok(entry) => entry,
                Err(_) => return WalkState::Continue,
            };

            if is_target_file(&entry) {
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

fn is_target_file(entry: &DirEntry) -> bool {
    entry
        .file_type()
        .is_some_and(|file_type| file_type.is_file())
        && entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| TARGET_EXTENSIONS.contains(&extension))
}
