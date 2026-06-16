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

pub fn compress_file(path: &Path, requested_level: CompressionLevel) -> Result<FileVariants> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("cannot read source file {}", path.display()))?;
    let language = language_for_path(path)?;
    let syntax = SyntaxKind::from_path(path)?;

    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .context("cannot load parser")?;
    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| anyhow!("tree-sitter parse failed"))?;

    let full = (requested_level == CompressionLevel::Full).then(|| source.clone());
    let skeleton = strip_to_skeleton(&source, tree.root_node(), syntax);
    let tree_map = build_tree_map(&source, tree.root_node(), syntax);

    Ok(FileVariants {
        full,
        skeleton,
        tree_map,
    })
}

fn language_for_path(path: &Path) -> Result<Language> {
    match extension(path).as_deref() {
        Some("js" | "jsx") => Ok(tree_sitter_javascript::language()),
        Some("ts" | "tsx") => Ok(tree_sitter_typescript::language_typescript()),
        Some("py") => Ok(tree_sitter_python::language()),
        Some("rs") => Ok(tree_sitter_rust::language()),
        Some(other) => bail!("unsupported extension: {other}"),
        None => bail!("file has no extension: {}", path.display()),
    }
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
}

impl SyntaxKind {
    fn from_path(path: &Path) -> Result<Self> {
        match extension(path).as_deref() {
            Some("js" | "jsx") => Ok(Self::JavaScript),
            Some("ts" | "tsx") => Ok(Self::TypeScript),
            Some("py") => Ok(Self::Python),
            Some("rs") => Ok(Self::Rust),
            Some(other) => bail!("unsupported extension: {other}"),
            None => bail!("file has no extension: {}", path.display()),
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
    }
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

    if line.len() > 240 {
        line.truncate(237);
        line.push_str("...");
    }

    (!line.is_empty()).then_some(line)
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
