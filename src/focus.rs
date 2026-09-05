use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::budget::{ProcessedFile, TokenCounter};
use crate::parser::{declared_symbol_names, CompressionLevel};

const MAX_FOCUS_TERMS: usize = 16;

const FOCUS_STOP_WORDS: &[&str] = &[
    "about",
    "after",
    "also",
    "and",
    "before",
    "can",
    "change",
    "code",
    "create",
    "file",
    "files",
    "fix",
    "for",
    "from",
    "get",
    "give",
    "how",
    "into",
    "make",
    "module",
    "modules",
    "need",
    "please",
    "project",
    "repo",
    "repository",
    "show",
    "tell",
    "that",
    "the",
    "this",
    "use",
    "what",
    "when",
    "where",
    "which",
    "with",
    "why",
];

const CANDIDATE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "py", "rs", "go", "java", "cs", "swift", "kt",
];

/// Request-aware selection: keep modules matching the request (plus their
/// dependencies and nearby tests) at higher detail while shrinking background
/// files. Mirrors the VS Code extension engine. Level changes only apply to
/// explicit lossy levels; at level 1 every file keeps full source and the
/// request only affects priority scores.
pub fn apply_request_focus(
    files: &mut [ProcessedFile],
    requested_level: CompressionLevel,
    focus: Option<&str>,
    counter: &TokenCounter,
) {
    let terms = extract_focus_terms(focus);
    if terms.is_empty() {
        return;
    }

    let adjust_levels = requested_level != CompressionLevel::Full;

    let mut relevances = Vec::with_capacity(files.len());
    for file in files.iter() {
        let source = focus_source(file);
        let symbols = declared_symbol_names(source, Path::new(file.path.as_str()));
        let relevance = task_relevance(&file.path, source, &symbols, &terms);
        relevances.push(relevance);
    }

    for (file, relevance) in files.iter_mut().zip(relevances.iter()) {
        file.task_boost = relevance * 4_000;
        if adjust_levels {
            file.level = initial_file_level(requested_level, *relevance);
        }
    }

    let graph = build_dependency_graph(files);
    let (distances, levels) = find_task_context(files, &graph);

    for file in files.iter_mut() {
        let Some(distance) = distances.get(file.path.as_str()).copied() else {
            continue;
        };
        match distance {
            1 => file.task_boost += 2_200,
            2 => file.task_boost += 1_000,
            _ => {}
        }
        if adjust_levels {
            if let Some(related_level) = levels.get(file.path.as_str()).copied() {
                if related_level < file.level {
                    file.level = related_level;
                }
            }
        }
    }

    for file in files.iter_mut() {
        file.token_count = counter.count(file.content());
    }
}

fn focus_source(file: &ProcessedFile) -> &str {
    file.variants.full.as_deref().unwrap_or(file.content())
}

pub fn extract_focus_terms(focus: Option<&str>) -> Vec<String> {
    let Some(focus) = focus else {
        return Vec::new();
    };

    let mut terms = Vec::new();
    for term in focus
        .to_ascii_lowercase()
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        if term.len() < 3 || FOCUS_STOP_WORDS.contains(&term) || terms.contains(&term.to_owned()) {
            continue;
        }
        terms.push(term.to_owned());
        if terms.len() >= MAX_FOCUS_TERMS {
            break;
        }
    }
    terms
}

fn task_relevance(relative_path: &str, source: &str, symbols: &[String], terms: &[String]) -> i64 {
    if terms.is_empty() {
        return 0;
    }

    let path_text = relative_path.to_ascii_lowercase();
    let source_text = source.to_ascii_lowercase();
    let symbol_text = symbols.join(" ").to_ascii_lowercase();

    let mut score = 0i64;
    for term in terms {
        if path_text.contains(term.as_str()) {
            score += 6;
        } else if symbol_text.contains(term.as_str()) {
            score += 5;
        } else if source_text.contains(term.as_str()) {
            score += 2;
        }
    }
    score
}

fn initial_file_level(requested: CompressionLevel, relevance: i64) -> CompressionLevel {
    if relevance > 0 {
        initial_level(requested, relevance)
    } else {
        background_level(requested)
    }
}

fn initial_level(requested: CompressionLevel, relevance: i64) -> CompressionLevel {
    if relevance >= 5 {
        match requested {
            CompressionLevel::Full => CompressionLevel::Full,
            CompressionLevel::Skeleton => CompressionLevel::Full,
            CompressionLevel::TreeMap => CompressionLevel::Skeleton,
        }
    } else {
        requested
    }
}

fn background_level(requested: CompressionLevel) -> CompressionLevel {
    match requested {
        CompressionLevel::Full => CompressionLevel::Skeleton,
        CompressionLevel::Skeleton => CompressionLevel::TreeMap,
        CompressionLevel::TreeMap => CompressionLevel::TreeMap,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ImportKind {
    Module,
    RustModule,
    RustUse,
    Include,
}

#[derive(Debug, Clone)]
struct ImportReference {
    kind: ImportKind,
    value: String,
}

fn extract_import_references(relative_path: &str, source: &str) -> Vec<ImportReference> {
    let extension = relative_path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut references = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('*')
            || trimmed.starts_with("<!--")
        {
            continue;
        }

        match extension.as_str() {
            "js" | "jsx" | "ts" | "tsx" => {
                if line.contains("from") || line.contains("import") || line.contains("require") {
                    for value in quoted_values(line) {
                        references.push(ImportReference {
                            kind: ImportKind::Module,
                            value,
                        });
                    }
                }
            }
            "py" => {
                if let Some(rest) = trimmed.strip_prefix("from ") {
                    let mut parts = rest.split_whitespace();
                    let module = parts.next().unwrap_or_default();
                    let imported = rest
                        .split("import")
                        .nth(1)
                        .unwrap_or_default()
                        .split(',')
                        .next()
                        .unwrap_or_default()
                        .trim();
                    if module.starts_with('.') && !imported.is_empty() {
                        references.push(ImportReference {
                            kind: ImportKind::Module,
                            value: format!("{module}{imported}"),
                        });
                    }
                    if !module.is_empty() {
                        references.push(ImportReference {
                            kind: ImportKind::Module,
                            value: module.to_owned(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("import ") {
                    let module = rest
                        .split(',')
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .trim_end_matches(',');
                    if !module.is_empty() {
                        references.push(ImportReference {
                            kind: ImportKind::Module,
                            value: module.to_owned(),
                        });
                    }
                }
            }
            "rs" => {
                if trimmed.starts_with("mod ") && trimmed.ends_with(';') {
                    let name = trimmed[4..trimmed.len() - 1].trim();
                    if is_identifier(name) {
                        references.push(ImportReference {
                            kind: ImportKind::RustModule,
                            value: name.to_owned(),
                        });
                    }
                }
                if let Some(value) = use_path(trimmed) {
                    references.push(ImportReference {
                        kind: ImportKind::RustUse,
                        value,
                    });
                }
            }
            "c" | "h" | "cpp" | "hpp" | "m" | "mm" => {
                if trimmed.starts_with("#include") || trimmed.starts_with("#import") {
                    if let Some(value) = bracketed_value(trimmed) {
                        references.push(ImportReference {
                            kind: ImportKind::Include,
                            value,
                        });
                    }
                }
            }
            _ => {
                if trimmed.starts_with("import ")
                    || trimmed.starts_with("using ")
                    || trimmed.starts_with("#import ")
                {
                    let rest = trimmed
                        .split_once(' ')
                        .map(|(_, rest)| rest)
                        .unwrap_or_default()
                        .trim()
                        .trim_start_matches(|ch| ch == '<' || ch == '"' || ch == '\'');
                    let end = rest
                        .find(|ch: char| {
                            ch.is_whitespace() || ch == ';' || ch == '"' || ch == '>' || ch == '\''
                        })
                        .unwrap_or(rest.len());
                    let value = rest[..end].trim().to_owned();
                    if !value.is_empty() {
                        references.push(ImportReference {
                            kind: ImportKind::Module,
                            value,
                        });
                    }
                }
            }
        }
    }

    let mut seen = HashSet::new();
    references
        .into_iter()
        .filter(|reference| seen.insert((reference.kind, reference.value.clone())))
        .collect()
}

fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let quote = bytes[index];
        if quote != b'\'' && quote != b'"' && quote != b'`' {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < bytes.len() && bytes[end] != quote {
            if bytes[end] == b'\\' {
                end += 1;
            }
            end += 1;
        }
        if end < bytes.len() {
            let value = line[index + 1..end].trim().to_owned();
            if !value.is_empty() {
                values.push(value);
            }
            index = end + 1;
        } else {
            break;
        }
    }
    values
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !value.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}

fn use_path(line: &str) -> Option<String> {
    let mut search_from = 0;
    while let Some(offset) = line[search_from..].find("use ") {
        let start = search_from + offset;
        let preceded_ok = start == 0
            || line[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        if preceded_ok {
            let rest = &line[start + 4..];
            let end = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
                .unwrap_or(rest.len());
            let mut value = rest[..end].trim_end_matches(':').to_owned();
            if !value.is_empty() {
                if value.ends_with("::") {
                    value.truncate(value.len() - 2);
                }
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
        search_from = start + 4;
    }
    None
}

fn bracketed_value(line: &str) -> Option<String> {
    let start = line.find(|ch| ch == '<' || ch == '"')?;
    let open = line.as_bytes()[start] as char;
    let close = if open == '<' { '>' } else { open };
    let rest = &line[start + 1..];
    let end = rest.find(close)?;
    let value = rest[..end].trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn resolve_import_path(
    from_path: &str,
    reference: &ImportReference,
    paths: &HashSet<String>,
) -> Option<String> {
    let extension = from_path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let normalized = reference
        .value
        .strip_suffix("::")
        .unwrap_or(reference.value.as_str());

    let bases: Vec<String> = match reference.kind {
        ImportKind::RustModule => vec![posix_join(&posix_dirname(from_path), normalized)],
        ImportKind::RustUse => rust_use_candidates(from_path, normalized),
        ImportKind::Include => vec![
            posix_join(&posix_dirname(from_path), normalized),
            normalized.to_owned(),
            posix_join("include", normalized),
        ],
        ImportKind::Module => {
            if normalized.starts_with('.') && extension == "py" {
                let dots = normalized
                    .chars()
                    .take_while(|ch| *ch == '.')
                    .count()
                    .max(1);
                let mut directory = posix_dirname(from_path);
                for _ in 1..dots {
                    directory = posix_dirname(&directory);
                }
                vec![posix_join(
                    &directory,
                    &normalized[dots..].replace('.', "/"),
                )]
            } else if normalized.starts_with('.') {
                vec![posix_normalize(&posix_join(
                    &posix_dirname(from_path),
                    normalized,
                ))]
            } else {
                vec![normalized.replace("::", "/").replace('.', "/")]
            }
        }
    };

    let mut candidates = Vec::new();
    for base in bases {
        candidates.push(base.clone());
        for ext in CANDIDATE_EXTENSIONS {
            candidates.push(format!("{base}.{ext}"));
        }
        for ext in CANDIDATE_EXTENSIONS {
            candidates.push(format!("{base}/index.{ext}"));
        }
        candidates.push(format!("{base}/mod.rs"));
    }
    candidates
        .into_iter()
        .find(|candidate| paths.contains(candidate))
}

fn rust_use_candidates(from_path: &str, reference: &str) -> Vec<String> {
    if let Some(rest) = reference.strip_prefix("crate::") {
        return rust_module_prefixes(&posix_join(
            &rust_source_root(from_path),
            &rest.replace("::", "/"),
        ));
    }
    let directory = posix_dirname(from_path);
    if let Some(rest) = reference.strip_prefix("self::") {
        return rust_module_prefixes(&posix_join(&directory, &rest.replace("::", "/")));
    }
    if reference.starts_with("super::") {
        let mut parent = directory;
        let mut remaining = reference;
        while let Some(rest) = remaining.strip_prefix("super::") {
            parent = posix_dirname(&parent);
            remaining = rest;
        }
        return rust_module_prefixes(&posix_join(&parent, &remaining.replace("::", "/")));
    }
    rust_module_prefixes(&posix_join(&directory, &reference.replace("::", "/")))
}

fn rust_source_root(from_path: &str) -> String {
    let segments: Vec<&str> = from_path.split('/').collect();
    if let Some(index) = segments.iter().rposition(|segment| *segment == "src") {
        return segments[..=index].join("/");
    }
    "src".to_owned()
}

fn rust_module_prefixes(value: &str) -> Vec<String> {
    let segments: Vec<&str> = value.split('/').filter(|part| !part.is_empty()).collect();
    (0..segments.len())
        .map(|drop| segments[..segments.len() - drop].join("/"))
        .collect()
}

fn posix_dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(index) => {
            if index == 0 {
                "/".to_owned()
            } else {
                path[..index].to_owned()
            }
        }
        None => ".".to_owned(),
    }
}

fn posix_join(directory: &str, name: &str) -> String {
    if directory.is_empty() || directory == "." {
        name.to_owned()
    } else {
        format!("{directory}/{name}")
    }
}

fn posix_normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

fn is_test_path(relative_path: &str) -> bool {
    let lower = relative_path.to_ascii_lowercase();
    let segments: Vec<&str> = lower.split('/').collect();
    // A path segment that is exactly test/tests/spec/specs/__tests__ marks a
    // test directory (the file name itself always has an extension, so an
    // exact match can only be a directory except for extensionless paths).
    for (index, segment) in segments.iter().enumerate() {
        if matches!(*segment, "test" | "tests" | "spec" | "specs" | "__tests__")
            && (index + 1 < segments.len() || segments.len() == 1)
        {
            return true;
        }
    }

    let file_name = segments.last().copied().unwrap_or_default();
    let without_extension = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    without_extension.starts_with("test_")
        || without_extension.starts_with("spec_")
        || without_extension.ends_with(".test")
        || without_extension.ends_with(".spec")
        || without_extension.ends_with("_test")
        || without_extension.ends_with("_spec")
}

fn file_stem(relative_path: &str, strip_affixes: bool) -> String {
    let file_name = relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .to_ascii_lowercase();
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name.as_str())
        .to_owned();
    if !strip_affixes {
        return stem;
    }
    let stem = stem
        .strip_prefix("test_")
        .or_else(|| stem.strip_prefix("spec_"))
        .unwrap_or(stem.as_str());
    stem.strip_suffix(".test")
        .or_else(|| stem.strip_suffix(".spec"))
        .or_else(|| stem.strip_suffix("_test"))
        .or_else(|| stem.strip_suffix("_spec"))
        .unwrap_or(stem)
        .to_owned()
}

fn is_likely_test_pair(test_path: &str, source_path: &str) -> bool {
    let test_directory = posix_dirname(test_path);
    let source_directory = posix_dirname(source_path);
    if test_directory == source_directory {
        return true;
    }

    let test_segments: Vec<&str> = test_directory
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let mut marker_index: Option<usize> = None;
    for (index, segment) in test_segments.iter().enumerate() {
        if matches!(
            segment.to_ascii_lowercase().as_str(),
            "__tests__" | "tests" | "test" | "spec" | "specs"
        ) {
            marker_index = Some(index);
        }
    }
    if let Some(marker) = marker_index {
        let context = test_segments[marker + 1..].join("/");
        if !context.is_empty() {
            return source_directory == context
                || source_directory.ends_with(&format!("/{context}"));
        }
        return matches!(source_directory.as_str(), "." | "src" | "lib")
            || posix_dirname(&test_directory) == source_directory;
    }

    false
}

fn build_dependency_graph(files: &[ProcessedFile]) -> HashMap<String, HashSet<String>> {
    let paths: HashSet<String> = files.iter().map(|file| file.path.clone()).collect();
    let mut graph: HashMap<String, HashSet<String>> = files
        .iter()
        .map(|file| (file.path.clone(), HashSet::new()))
        .collect();

    for file in files {
        let source = focus_source(file);
        for reference in extract_import_references(&file.path, source) {
            let Some(target) = resolve_import_path(&file.path, &reference, &paths) else {
                continue;
            };
            if target == file.path {
                continue;
            }
            if let Some(edges) = graph.get_mut(file.path.as_str()) {
                edges.insert(target.clone());
            }
            if let Some(edges) = graph.get_mut(target.as_str()) {
                edges.insert(file.path.clone());
            }
        }
    }
    connect_matching_tests(files, &mut graph);
    graph
}

fn connect_matching_tests(files: &[ProcessedFile], graph: &mut HashMap<String, HashSet<String>>) {
    let mut by_stem: HashMap<String, Vec<String>> = HashMap::new();
    for file in files.iter().filter(|file| !is_test_path(&file.path)) {
        by_stem
            .entry(file_stem(&file.path, false))
            .or_default()
            .push(file.path.clone());
    }
    for test_file in files.iter().filter(|file| is_test_path(&file.path)) {
        let stem = file_stem(&test_file.path, true);
        if stem.is_empty() {
            continue;
        }
        let Some(candidates) = by_stem.get(&stem).cloned() else {
            continue;
        };
        for source_path in candidates {
            if is_likely_test_pair(&test_file.path, &source_path) {
                if let Some(edges) = graph.get_mut(test_file.path.as_str()) {
                    edges.insert(source_path.clone());
                }
                if let Some(edges) = graph.get_mut(source_path.as_str()) {
                    edges.insert(test_file.path.clone());
                }
            }
        }
    }
}

fn find_task_context(
    files: &[ProcessedFile],
    graph: &HashMap<String, HashSet<String>>,
) -> (HashMap<String, usize>, HashMap<String, CompressionLevel>) {
    let mut distances: HashMap<String, usize> = HashMap::new();
    let mut levels: HashMap<String, CompressionLevel> = HashMap::new();
    let mut queue: Vec<String> = Vec::new();

    for file in files {
        if file.task_boost > 0 {
            distances.insert(file.path.clone(), 0);
            levels.insert(file.path.clone(), file.level);
            queue.push(file.path.clone());
        }
    }

    let mut index = 0;
    while index < queue.len() {
        let current = queue[index].clone();
        index += 1;
        let distance = distances.get(&current).copied().unwrap_or(0);
        if distance >= 2 {
            continue;
        }
        let level = levels
            .get(&current)
            .copied()
            .unwrap_or(CompressionLevel::TreeMap);
        let neighbors: Vec<String> = graph
            .get(&current)
            .map(|edges| edges.iter().cloned().collect())
            .unwrap_or_default();
        for neighbor in neighbors {
            let next_distance = distance + 1;
            let next_level = match (level as u8) + 1 {
                1 => CompressionLevel::Full,
                2 => CompressionLevel::Skeleton,
                _ => CompressionLevel::TreeMap,
            };
            let dominated = match distances.get(&neighbor).copied() {
                None => false,
                Some(previous) => {
                    previous < next_distance
                        || (previous == next_distance
                            && levels
                                .get(&neighbor)
                                .copied()
                                .unwrap_or(CompressionLevel::TreeMap)
                                as u8
                                <= next_level as u8)
                }
            };
            if dominated {
                continue;
            }
            distances.insert(neighbor.clone(), next_distance);
            levels.insert(neighbor.clone(), next_level);
            queue.push(neighbor);
        }
    }

    (distances, levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::TokenizerKind;
    use crate::parser::FileVariants;

    fn test_counter() -> TokenCounter {
        TokenCounter::new(TokenizerKind::default()).unwrap()
    }

    fn test_file(path: &str, source: &str) -> ProcessedFile {
        test_file_at(path, source, CompressionLevel::Skeleton)
    }

    fn test_file_at(path: &str, source: &str, level: CompressionLevel) -> ProcessedFile {
        ProcessedFile::new(
            path.to_owned(),
            level,
            FileVariants {
                full: Some(source.to_owned()),
                skeleton: format!("{source}\n... skeleton"),
                tree_map: path.to_owned(),
            },
        )
    }

    #[test]
    fn extracts_request_terms_without_stop_words() {
        let terms = extract_focus_terms(Some("Please fix the authentication flow!"));
        assert_eq!(terms, vec!["authentication", "flow"]);
    }

    #[test]
    fn scores_path_symbol_and_source_matches() {
        let terms = vec!["billing".to_owned()];
        assert_eq!(
            task_relevance("src/billing.rs", "fn x() {}", &[], &terms),
            6
        );
        assert_eq!(
            task_relevance(
                "src/lib.rs",
                "fn x() {}",
                &["charge_billing".to_owned()],
                &terms
            ),
            5
        );
        assert_eq!(
            task_relevance("src/lib.rs", "billing enabled", &[], &terms),
            2
        );
        assert_eq!(task_relevance("src/lib.rs", "nothing", &[], &terms), 0);
    }

    #[test]
    fn keeps_relevant_and_dependencies_above_background() {
        let mut files = vec![
            test_file(
                "src/auth.ts",
                "import { token } from './token'\nexport function authenticate() { return token() }",
            ),
            test_file("src/token.ts", "export function token() { return 1 }"),
            test_file("src/unrelated.ts", "export const value = 1"),
        ];
        let counter = test_counter();
        apply_request_focus(
            &mut files,
            CompressionLevel::Skeleton,
            Some("fix authenticate"),
            &counter,
        );

        let level = |path: &str| files.iter().find(|file| file.path == path).unwrap().level;
        assert_eq!(level("src/auth.ts"), CompressionLevel::Full);
        assert_eq!(level("src/token.ts"), CompressionLevel::Skeleton);
        assert_eq!(level("src/unrelated.ts"), CompressionLevel::TreeMap);

        let boost = |path: &str| {
            files
                .iter()
                .find(|file| file.path == path)
                .unwrap()
                .task_boost
        };
        assert!(boost("src/auth.ts") > boost("src/token.ts"));
        assert!(boost("src/token.ts") > boost("src/unrelated.ts"));
    }

    #[test]
    fn level_one_never_shrinks_background_for_focus() {
        let mut files = vec![
            test_file_at(
                "src/auth.ts",
                "export function authenticate() { return true }",
                CompressionLevel::Full,
            ),
            test_file_at(
                "src/unrelated.ts",
                "export const value = 1",
                CompressionLevel::Full,
            ),
        ];
        let counter = test_counter();
        apply_request_focus(
            &mut files,
            CompressionLevel::Full,
            Some("fix authenticate"),
            &counter,
        );

        assert!(files
            .iter()
            .all(|file| file.level == CompressionLevel::Full));
        assert!(
            files
                .iter()
                .find(|file| file.path == "src/auth.ts")
                .unwrap()
                .task_boost
                > 0
        );
    }

    #[test]
    fn resolves_relative_imports_and_test_pairs() {
        let paths: HashSet<String> = [
            "src/auth.ts",
            "src/session.ts",
            "src/auth.test.ts",
            "src/index.ts",
        ]
        .iter()
        .map(|path| (*path).to_owned())
        .collect();

        let reference = ImportReference {
            kind: ImportKind::Module,
            value: "./auth".to_owned(),
        };
        assert_eq!(
            resolve_import_path("src/session.ts", &reference, &paths).as_deref(),
            Some("src/auth.ts")
        );
        assert!(is_test_path("src/auth.test.ts"));
        assert!(!is_test_path("src/auth.ts"));
        assert!(is_likely_test_pair("src/auth.test.ts", "src/auth.ts"));
    }
}
