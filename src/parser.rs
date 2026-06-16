use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompressionLevel {
    Full = 1,
    Skeleton = 2,
    TreeMap = 3,
}

#[derive(Debug, Clone)]
pub struct FileVariants {
    pub full: Option<String>,
    pub skeleton: String,
    pub tree_map: String,
}

impl CompressionLevel {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for CompressionLevel {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Full),
            2 => Ok(Self::Skeleton),
            3 => Ok(Self::TreeMap),
            _ => bail!("level must be 1, 2, or 3"),
        }
    }
}

pub fn compress_file(path: &Path, _requested_level: CompressionLevel) -> Result<FileVariants> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("cannot read source file {}", path.display()))?;
    let syntax = SyntaxKind::from_path(path)?;

    let full = Some(source.clone());
    let (skeleton, tree_map) = match syntax.language() {
        Some(language) => {
            let mut parser = Parser::new();
            parser
                .set_language(&language)
                .context("cannot load parser")?;
            let tree = parser
                .parse(&source, None)
                .ok_or_else(|| anyhow!("tree-sitter parse failed"))?;

            (
                strip_to_skeleton(&source, tree.root_node(), syntax),
                build_tree_map(&source, tree.root_node(), syntax),
            )
        }
        None => match syntax {
            SyntaxKind::GenericBrace => (
                strip_generic_brace_code(&source),
                build_generic_brace_tree_map(&source),
            ),
            SyntaxKind::Text => (
                compact_text_context(path, &source),
                build_text_tree_map(path, &source),
            ),
            _ => unreachable!("tree-sitter language missing for parsed syntax"),
        },
    };

    Ok(FileVariants {
        full,
        skeleton,
        tree_map,
    })
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntaxKind {
    JavaScript,
    TypeScript,
    Python,
    Rust,
    GenericBrace,
    Text,
}

impl SyntaxKind {
    fn from_path(path: &Path) -> Result<Self> {
        match extension(path).as_deref() {
            Some("js" | "jsx") => Ok(Self::JavaScript),
            Some("ts" | "tsx") => Ok(Self::TypeScript),
            Some("py") => Ok(Self::Python),
            Some("rs") => Ok(Self::Rust),
            Some("go" | "java" | "cs" | "swift" | "kt") => Ok(Self::GenericBrace),
            Some("md" | "json" | "yaml" | "yml" | "toml") => Ok(Self::Text),
            Some(other) => bail!("unsupported extension: {other}"),
            None => bail!("file has no extension: {}", path.display()),
        }
    }

    fn language(self) -> Option<Language> {
        match self {
            Self::JavaScript => Some(tree_sitter_javascript::language()),
            Self::TypeScript => Some(tree_sitter_typescript::language_typescript()),
            Self::Python => Some(tree_sitter_python::language()),
            Self::Rust => Some(tree_sitter_rust::language()),
            Self::GenericBrace | Self::Text => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Replacement {
    start: usize,
    end: usize,
    value: &'static str,
}

fn strip_to_skeleton(source: &str, root: Node, syntax: SyntaxKind) -> String {
    let mut replacements = Vec::new();
    collect_body_replacements(root, syntax, &mut replacements);
    apply_replacements(source, replacements)
}

fn collect_body_replacements(node: Node, syntax: SyntaxKind, replacements: &mut Vec<Replacement>) {
    if let Some(replacement) = body_replacement(node, syntax) {
        replacements.push(replacement);
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_body_replacements(child, syntax, replacements);
    }
}

fn body_replacement(node: Node, syntax: SyntaxKind) -> Option<Replacement> {
    match syntax {
        SyntaxKind::JavaScript | SyntaxKind::TypeScript => {
            let kind = node.kind();
            if kind == "statement_block" && node.parent().is_some_and(is_js_callable) {
                Some(Replacement {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    value: "{ ... }",
                })
            } else if kind == "class_body"
                && node.parent().is_some_and(is_js_anonymous_export_class)
            {
                None
            } else {
                None
            }
        }
        SyntaxKind::Python => {
            if node.kind() == "block" && node.parent().is_some_and(is_python_callable) {
                Some(Replacement {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    value: "...",
                })
            } else {
                None
            }
        }
        SyntaxKind::Rust => {
            if node.kind() == "block" && node.parent().is_some_and(is_rust_callable) {
                Some(Replacement {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    value: "{ ... }",
                })
            } else {
                None
            }
        }
        SyntaxKind::GenericBrace | SyntaxKind::Text => None,
    }
}

fn is_js_callable(node: Node) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "function"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration"
    )
}

fn is_js_anonymous_export_class(node: Node) -> bool {
    node.kind() == "class_declaration"
}

fn is_python_callable(node: Node) -> bool {
    matches!(node.kind(), "function_definition" | "decorated_definition")
}

fn is_rust_callable(node: Node) -> bool {
    matches!(node.kind(), "function_item" | "closure_expression")
}

fn apply_replacements(source: &str, mut replacements: Vec<Replacement>) -> String {
    if replacements.is_empty() {
        return source.to_owned();
    }

    replacements.sort_unstable_by_key(|replacement| replacement.start);
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;

    for replacement in replacements {
        if replacement.start < cursor {
            continue;
        }

        output.push_str(&source[cursor..replacement.start]);
        output.push_str(replacement.value);
        cursor = replacement.end;
    }

    output.push_str(&source[cursor..]);
    output
}

fn build_tree_map(source: &str, root: Node, syntax: SyntaxKind) -> String {
    let mut lines = Vec::new();
    collect_tree_map_lines(source, root, syntax, &mut lines);

    if lines.is_empty() {
        return String::new();
    }

    dedupe_preserving_order(lines).join("\n")
}

fn collect_tree_map_lines(source: &str, node: Node, syntax: SyntaxKind, lines: &mut Vec<String>) {
    if should_emit_tree_map_node(node, syntax) {
        if let Some(line) = compact_node_line(source, node) {
            lines.push(line);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_tree_map_lines(source, child, syntax, lines);
    }
}

fn should_emit_tree_map_node(node: Node, syntax: SyntaxKind) -> bool {
    let kind = node.kind();
    match syntax {
        SyntaxKind::JavaScript => matches!(
            kind,
            "import_statement"
                | "export_statement"
                | "function_declaration"
                | "class_declaration"
                | "method_definition"
                | "lexical_declaration"
                | "variable_declaration"
        ),
        SyntaxKind::TypeScript => matches!(
            kind,
            "import_statement"
                | "export_statement"
                | "function_declaration"
                | "class_declaration"
                | "method_definition"
                | "interface_declaration"
                | "type_alias_declaration"
                | "enum_declaration"
                | "ambient_declaration"
                | "lexical_declaration"
                | "variable_declaration"
        ),
        SyntaxKind::Python => matches!(
            kind,
            "import_statement"
                | "import_from_statement"
                | "function_definition"
                | "class_definition"
                | "decorated_definition"
        ),
        SyntaxKind::Rust => matches!(
            kind,
            "use_declaration"
                | "function_item"
                | "struct_item"
                | "enum_item"
                | "trait_item"
                | "impl_item"
                | "type_item"
                | "const_item"
                | "static_item"
                | "mod_item"
        ),
        SyntaxKind::GenericBrace | SyntaxKind::Text => false,
    }
}

fn strip_generic_brace_code(source: &str) -> String {
    let mut output = Vec::new();
    let mut depth = 0usize;

    for line in source.lines() {
        let trimmed = line.trim();
        let line_depth = depth;

        if line_depth == 0 && should_keep_generic_code_line(trimmed) {
            output.push(collapse_generic_body(trimmed));
        }

        depth = update_brace_depth(depth, line);
    }

    if output.is_empty() {
        compact_non_empty_lines(source, 120)
    } else {
        dedupe_preserving_order(output).join("\n")
    }
}

fn build_generic_brace_tree_map(source: &str) -> String {
    let lines = source
        .lines()
        .map(str::trim)
        .filter(|line| should_keep_generic_code_line(line))
        .map(collapse_generic_body)
        .collect::<Vec<_>>();

    if lines.is_empty() {
        compact_non_empty_lines(source, 80)
    } else {
        dedupe_preserving_order(lines).join("\n")
    }
}

fn should_keep_generic_code_line(line: &str) -> bool {
    if line.is_empty() || line.starts_with("//") {
        return false;
    }

    matches!(
        line.split_whitespace().next(),
        Some(
            "package"
                | "import"
                | "using"
                | "namespace"
                | "public"
                | "private"
                | "protected"
                | "internal"
                | "class"
                | "struct"
                | "interface"
                | "enum"
                | "protocol"
                | "extension"
                | "func"
                | "fun"
                | "var"
                | "let"
                | "const"
        )
    ) || line.contains(" class ")
        || line.contains(" struct ")
        || line.contains(" interface ")
        || line.contains(" enum ")
        || line.contains(" func ")
        || line.contains(" fun ")
}

fn collapse_generic_body(line: &str) -> String {
    let mut collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(index) = collapsed.find('{') {
        collapsed.truncate(index);
        collapsed = collapsed.trim_end().to_owned();
        collapsed.push_str(" { ... }");
    }
    truncate_line(collapsed, 240)
}

fn update_brace_depth(depth: usize, line: &str) -> usize {
    line.chars()
        .fold(depth, |depth, character| match character {
            '{' => depth.saturating_add(1),
            '}' => depth.saturating_sub(1),
            _ => depth,
        })
}

fn compact_text_context(path: &Path, source: &str) -> String {
    if extension(path).as_deref() == Some("md") {
        let lines = source
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.starts_with('#')
                    || line.starts_with("- ")
                    || line.starts_with("* ")
                    || line.starts_with("> ")
            })
            .map(|line| truncate_line(line.to_owned(), 240))
            .take(160)
            .collect::<Vec<_>>();

        if !lines.is_empty() {
            return lines.join("\n");
        }
    }

    compact_non_empty_lines(source, 160)
}

fn build_text_tree_map(path: &Path, source: &str) -> String {
    match extension(path).as_deref() {
        Some("md") => {
            let headings = source
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with('#'))
                .map(|line| truncate_line(line.to_owned(), 240))
                .take(120)
                .collect::<Vec<_>>();

            if headings.is_empty() {
                compact_non_empty_lines(source, 80)
            } else {
                headings.join("\n")
            }
        }
        Some("json" | "yaml" | "yml" | "toml") => compact_config_lines(source),
        _ => compact_non_empty_lines(source, 80),
    }
}

fn compact_config_lines(source: &str) -> String {
    source
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with("//")
                && !matches!(*line, "{" | "}" | "[" | "]" | "," | "},")
        })
        .map(|line| truncate_line(line.to_owned(), 240))
        .take(160)
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_non_empty_lines(source: &str, max_lines: usize) -> String {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| truncate_line(line.to_owned(), 240))
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_node_line(source: &str, node: Node) -> Option<String> {
    let start = node.start_byte();
    let end = node.end_byte();
    let text = source.get(start..end)?;
    let mut line = text
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    line = truncate_line(line, 240);

    (!line.is_empty()).then_some(line)
}

fn truncate_line(line: String, max_len: usize) -> String {
    if line.chars().count() <= max_len {
        return line;
    }

    let mut truncated = line
        .chars()
        .take(max_len.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn dedupe_preserving_order(lines: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::with_capacity(lines.len());
    for line in lines {
        if deduped.last() != Some(&line) {
            deduped.push(line);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rust_skeleton_strips_function_body() {
        let path = write_temp_source(
            "rs",
            r#"
use std::fmt;

fn greet(name: &str) -> String {
    format!("hello {name}")
}
"#,
        );

        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants
            .skeleton
            .contains("fn greet(name: &str) -> String { ... }"));
        assert!(!variants.skeleton.contains("format!"));
    }

    #[test]
    fn typescript_skeleton_strips_function_body() {
        let path = write_temp_source(
            "ts",
            r#"
import { readFile } from 'fs/promises'

export type User = { name: string }

export function greet(user: User): string {
    return `hello ${user.name}`
}
"#,
        );

        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants
            .skeleton
            .contains("export function greet(user: User): string { ... }"));
        assert!(variants.skeleton.contains("export type User"));
        assert!(!variants.skeleton.contains("return `hello"));
    }

    #[test]
    fn python_skeleton_strips_function_body() {
        let path = write_temp_source(
            "py",
            r#"
import os

def greet(name: str) -> str:
    return f"hello {name}"
"#,
        );

        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants.skeleton.contains("def greet(name: str) -> str:"));
        assert!(variants.skeleton.contains("..."));
        assert!(!variants.skeleton.contains("return f"));
    }

    #[test]
    fn tree_map_keeps_top_level_shapes() {
        let path = write_temp_source(
            "ts",
            r#"
import { readFile } from 'fs/promises'

interface User {
    name: string
}

class Greeter {
    greet(user: User): string {
        return user.name
    }
}
"#,
        );

        let variants = compress_file(&path, CompressionLevel::TreeMap).unwrap();

        assert!(variants
            .tree_map
            .contains("import { readFile } from 'fs/promises'"));
        assert!(variants.tree_map.contains("interface User"));
        assert!(variants.tree_map.contains("class Greeter"));
        assert!(variants.tree_map.contains("greet(user: User): string"));
        assert!(!variants.tree_map.contains("return user.name"));
    }

    fn write_temp_source(extension: &str, source: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("bonsai-parser-{unique}.{extension}"));
        fs::write(&path, source).unwrap();
        path
    }
}
