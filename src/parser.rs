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
    Go,
    Java,
    CSharp,
    Swift,
    Kotlin,
    Text,
}

impl SyntaxKind {
    fn from_path(path: &Path) -> Result<Self> {
        match extension(path).as_deref() {
            Some("js" | "jsx") => Ok(Self::JavaScript),
            Some("ts" | "tsx") => Ok(Self::TypeScript),
            Some("py") => Ok(Self::Python),
            Some("rs") => Ok(Self::Rust),
            Some("go") => Ok(Self::Go),
            Some("java") => Ok(Self::Java),
            Some("cs") => Ok(Self::CSharp),
            Some("swift") => Ok(Self::Swift),
            Some("kt") => Ok(Self::Kotlin),
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
            Self::Go => Some(tree_sitter_go::language()),
            Self::Java => Some(tree_sitter_java::language()),
            Self::CSharp => Some(tree_sitter_c_sharp::language()),
            Self::Swift => Some(tree_sitter_swift::language()),
            Self::Kotlin => Some(tree_sitter_kotlin::language()),
            Self::Text => None,
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
        SyntaxKind::Go
        | SyntaxKind::Java
        | SyntaxKind::CSharp
        | SyntaxKind::Swift
        | SyntaxKind::Kotlin => {
            if is_brace_body_node(node.kind())
                && node.parent().is_some_and(is_static_language_callable)
            {
                Some(Replacement {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    value: "{ ... }",
                })
            } else {
                None
            }
        }
        SyntaxKind::Text => None,
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

fn is_static_language_callable(node: Node) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "function_literal"
            | "lambda_expression"
            | "local_function_statement"
            | "function_value_parameters"
    )
}

fn is_brace_body_node(kind: &str) -> bool {
    matches!(
        kind,
        "block" | "constructor_body" | "function_body" | "code_block" | "control_structure_body"
    )
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
        SyntaxKind::Go => matches!(
            kind,
            "package_clause"
                | "import_declaration"
                | "const_declaration"
                | "var_declaration"
                | "type_declaration"
                | "function_declaration"
                | "method_declaration"
        ),
        SyntaxKind::Java => matches!(
            kind,
            "package_declaration"
                | "import_declaration"
                | "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
                | "method_declaration"
                | "constructor_declaration"
                | "field_declaration"
        ),
        SyntaxKind::CSharp => matches!(
            kind,
            "using_directive"
                | "namespace_declaration"
                | "class_declaration"
                | "struct_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "method_declaration"
                | "constructor_declaration"
                | "property_declaration"
                | "field_declaration"
                | "delegate_declaration"
        ),
        SyntaxKind::Swift => matches!(
            kind,
            "import_declaration"
                | "class_declaration"
                | "struct_declaration"
                | "protocol_declaration"
                | "extension_declaration"
                | "enum_declaration"
                | "function_declaration"
                | "property_declaration"
                | "typealias_declaration"
        ),
        SyntaxKind::Kotlin => matches!(
            kind,
            "package_header"
                | "import_header"
                | "class_declaration"
                | "object_declaration"
                | "function_declaration"
                | "property_declaration"
                | "typealias_declaration"
        ),
        SyntaxKind::Text => false,
    }
}

fn compact_text_context(path: &Path, source: &str) -> String {
    match extension(path).as_deref() {
        Some("md") => compact_markdown_context(source),
        Some("json" | "yaml" | "yml" | "toml") => compact_config_lines(source, 180),
        _ => compact_non_empty_lines(source, 160),
    }
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
        Some("json" | "yaml" | "yml" | "toml") => compact_config_lines(source, 100),
        _ => compact_non_empty_lines(source, 80),
    }
}

fn compact_markdown_context(source: &str) -> String {
    let mut output = Vec::new();
    let mut keep_after_heading = 0usize;
    let mut in_kept_fence = false;
    let mut kept_fence_lines = 0usize;
    let mut in_dropped_fence = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if in_kept_fence {
                output.push(trimmed.to_owned());
                in_kept_fence = false;
                continue;
            }
            if in_dropped_fence {
                in_dropped_fence = false;
                continue;
            }

            if is_important_fence(trimmed) {
                output.push(trimmed.to_owned());
                in_kept_fence = true;
                kept_fence_lines = 0;
            } else {
                in_dropped_fence = true;
            }
            continue;
        }

        if in_kept_fence {
            if kept_fence_lines < 24 {
                output.push(truncate_line(trimmed.to_owned(), 240));
            } else if kept_fence_lines == 24 {
                output.push("...".to_owned());
            }
            kept_fence_lines += 1;
            continue;
        }

        if in_dropped_fence || is_noisy_markdown_line(trimmed) {
            continue;
        }

        if trimmed.starts_with('#') {
            output.push(truncate_line(trimmed.to_owned(), 240));
            keep_after_heading = 2;
            continue;
        }

        if keep_after_heading > 0 && is_summary_markdown_line(trimmed) {
            output.push(truncate_line(trimmed.to_owned(), 240));
            keep_after_heading -= 1;
            continue;
        }

        if is_important_markdown_list_item(trimmed) {
            output.push(truncate_line(trimmed.to_owned(), 240));
        }
    }

    if output.is_empty() {
        compact_non_empty_lines(source, 120)
    } else {
        dedupe_preserving_order(output)
            .into_iter()
            .take(180)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn is_important_fence(line: &str) -> bool {
    let language = line
        .trim_start_matches("```")
        .trim_start_matches("~~~")
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();

    matches!(
        language.as_str(),
        "sh" | "bash"
            | "shell"
            | "text"
            | "txt"
            | "rust"
            | "rs"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "python"
            | "py"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
    )
}

fn is_noisy_markdown_line(line: &str) -> bool {
    line.is_empty()
        || line.contains("img.shields.io")
        || line.contains("<img ")
        || line.starts_with("<p align=")
        || (line.starts_with('|') && line.ends_with('|'))
        || line.chars().all(|ch| matches!(ch, '-' | ':' | '|' | ' '))
}

fn is_summary_markdown_line(line: &str) -> bool {
    !line.is_empty()
        && !line.starts_with('#')
        && !is_noisy_markdown_line(line)
        && line.chars().count() <= 280
}

fn is_important_markdown_list_item(line: &str) -> bool {
    (line.starts_with("- ") || line.starts_with("* "))
        && !line.contains("badge")
        && line.chars().count() <= 240
}

fn compact_config_lines(source: &str, max_lines: usize) -> String {
    let mut output = Vec::new();
    let mut kept_array_items = 0usize;
    let mut collapsed_array = false;

    for line in source.lines().map(str::trim) {
        if is_noisy_config_line(line) {
            continue;
        }

        if is_array_item(line) {
            kept_array_items += 1;
            if kept_array_items > 12 {
                if !collapsed_array {
                    output.push("...".to_owned());
                    collapsed_array = true;
                }
                continue;
            }
        } else if !line.starts_with(']') && !line.starts_with('}') {
            kept_array_items = 0;
            collapsed_array = false;
        }

        if is_top_level_config_line(line) || is_important_config_line(line) || kept_array_items > 0
        {
            output.push(truncate_line(line.to_owned(), 240));
        }

        if output.len() >= max_lines {
            break;
        }
    }

    if output.is_empty() {
        compact_non_empty_lines(source, max_lines)
    } else {
        dedupe_preserving_order(output).join("\n")
    }
}

fn is_noisy_config_line(line: &str) -> bool {
    line.is_empty()
        || line.starts_with('#')
        || line.starts_with("//")
        || matches!(line, "{" | "}" | "[" | "]" | "," | "}," | "],")
}

fn is_array_item(line: &str) -> bool {
    line.starts_with('"')
        || line.starts_with('{')
        || line.starts_with('-')
        || line.starts_with("[[")
        || line.ends_with(',')
}

fn is_top_level_config_line(line: &str) -> bool {
    line.starts_with('[')
        || line.starts_with("[[")
        || line.starts_with('"')
        || (!line.starts_with('-')
            && !line.starts_with(' ')
            && (line.contains(':') || line.contains('=')))
}

fn is_important_config_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "name",
        "version",
        "description",
        "scripts",
        "dependencies",
        "devdependencies",
        "peerdependencies",
        "workspaces",
        "jobs",
        "steps",
        "runs-on",
        "plugins",
        "skills",
        "contributes",
        "activationevents",
        "commands",
        "configuration",
        "package",
        "bin",
    ]
    .iter()
    .any(|key| lower.contains(key))
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
    fn static_language_parsers_keep_shapes() {
        let cases = [
            (
                "go",
                r#"
package demo

import "fmt"

func Greet(name string) string {
    return fmt.Sprintf("hello %s", name)
}
"#,
                "func Greet",
                "Sprintf",
            ),
            (
                "java",
                r#"
package demo;

import java.util.List;

public class Greeter {
    public String greet(String name) {
        return "hello " + name;
    }
}
"#,
                "class Greeter",
                "return \"hello",
            ),
            (
                "cs",
                r#"
using System;

namespace Demo;

public class Greeter {
    public string Greet(string name) {
        return $"hello {name}";
    }
}
"#,
                "class Greeter",
                "return $",
            ),
            (
                "swift",
                r#"
import Foundation

struct Greeter {
    func greet(name: String) -> String {
        return "hello \(name)"
    }
}
"#,
                "struct Greeter",
                "return \"hello",
            ),
            (
                "kt",
                r#"
package demo

class Greeter {
    fun greet(name: String): String {
        return "hello $name"
    }
}
"#,
                "class Greeter",
                "return \"hello",
            ),
        ];

        for (extension, source, expected_shape, body_text) in cases {
            let path = write_temp_source(extension, source);
            let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

            assert!(
                variants.tree_map.contains(expected_shape),
                "missing shape for {extension}: {}",
                variants.tree_map
            );
            assert!(
                !variants.skeleton.contains(body_text),
                "body kept for {extension}: {}",
                variants.skeleton
            );
        }
    }

    #[test]
    fn markdown_keeps_headings_summary_and_important_fences() {
        let path = write_temp_source(
            "md",
            r#"
<p align="center"><img src="badge.png" /></p>
| noisy | table |
| ----- | ----- |

# Bonsai

Useful summary text.
More summary.

```sh
bonsai .
```

```mermaid
graph TD
```
"#,
        );

        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants.skeleton.contains("# Bonsai"));
        assert!(variants.skeleton.contains("Useful summary text."));
        assert!(variants.skeleton.contains("```sh"));
        assert!(variants.skeleton.contains("bonsai ."));
        assert!(!variants.skeleton.contains("<img"));
        assert!(!variants.skeleton.contains("noisy | table"));
        assert!(!variants.skeleton.contains("mermaid"));
    }

    #[test]
    fn config_keeps_important_shape_and_collapses_long_arrays() {
        let path = write_temp_source(
            "json",
            r#"
{
  "name": "demo",
  "version": "0.1.0",
  "scripts": {
    "test": "cargo test"
  },
  "files": [
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n"
  ]
}
"#,
        );

        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants.skeleton.contains("\"name\": \"demo\""));
        assert!(variants.skeleton.contains("\"scripts\""));
        assert!(variants.skeleton.contains("\"test\": \"cargo test\""));
        assert!(variants.skeleton.contains("..."));
        assert!(!variants.skeleton.contains("\"n\""));
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
