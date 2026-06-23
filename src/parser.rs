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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserMode {
    TreeSitter,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserSupport {
    pub extension: String,
    pub mode: ParserMode,
    pub available: bool,
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

pub fn parser_support_for_extension(extension: &str) -> ParserSupport {
    let normalized = extension.trim_start_matches('.').to_ascii_lowercase();
    let syntax = SyntaxKind::from_extension(&normalized);
    let mode = match syntax {
        Ok(kind) if kind.language().is_some() => ParserMode::TreeSitter,
        Ok(_) => ParserMode::Compact,
        Err(_) => ParserMode::Compact,
    };
    let available = syntax
        .ok()
        .map(|kind| parser_available(kind))
        .unwrap_or(false);

    ParserSupport {
        extension: normalized,
        mode,
        available,
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
            SyntaxKind::Text | SyntaxKind::ObjectiveC | SyntaxKind::WebText => (
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
    C,
    Cpp,
    ObjectiveC,
    WebText,
    Text,
}

impl SyntaxKind {
    fn from_path(path: &Path) -> Result<Self> {
        match extension(path) {
            Some(extension) => Self::from_extension(&extension),
            None => bail!("file has no extension: {}", path.display()),
        }
    }

    fn from_extension(extension: &str) -> Result<Self> {
        match Some(extension) {
            Some("js" | "jsx") => Ok(Self::JavaScript),
            Some("ts" | "tsx") => Ok(Self::TypeScript),
            Some("py") => Ok(Self::Python),
            Some("rs") => Ok(Self::Rust),
            Some("go") => Ok(Self::Go),
            Some("java") => Ok(Self::Java),
            Some("cs") => Ok(Self::CSharp),
            Some("swift") => Ok(Self::Swift),
            Some("kt") => Ok(Self::Kotlin),
            Some("c" | "h") => Ok(Self::C),
            Some("cpp" | "hpp") => Ok(Self::Cpp),
            Some("m" | "mm") => Ok(Self::ObjectiveC),
            Some("vue" | "svelte" | "astro" | "html") => Ok(Self::WebText),
            Some("md" | "json" | "yaml" | "yml" | "toml") => Ok(Self::Text),
            Some(other) => bail!("unsupported extension: {other}"),
            None => unreachable!("extension is always provided"),
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
            Self::C => Some(tree_sitter_c::language()),
            Self::Cpp => Some(tree_sitter_cpp::language()),
            Self::ObjectiveC => None,
            Self::WebText => None,
            Self::Text => None,
        }
    }
}

fn parser_available(syntax: SyntaxKind) -> bool {
    let Some(language) = syntax.language() else {
        return true;
    };

    let mut parser = Parser::new();
    parser.set_language(&language).is_ok()
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
        | SyntaxKind::Kotlin
        | SyntaxKind::C
        | SyntaxKind::Cpp
        | SyntaxKind::ObjectiveC => {
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
        SyntaxKind::WebText | SyntaxKind::Text => None,
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
            | "method_definition"
            | "constructor_declaration"
            | "function_definition"
            | "function_literal"
            | "lambda_expression"
            | "local_function_statement"
            | "function_value_parameters"
    )
}

fn is_brace_body_node(kind: &str) -> bool {
    matches!(
        kind,
        "block"
            | "compound_statement"
            | "constructor_body"
            | "function_body"
            | "code_block"
            | "control_structure_body"
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
        SyntaxKind::C | SyntaxKind::Cpp => matches!(
            kind,
            "preproc_include"
                | "preproc_def"
                | "preproc_function_def"
                | "function_definition"
                | "declaration"
                | "type_definition"
                | "struct_specifier"
                | "union_specifier"
                | "enum_specifier"
                | "class_specifier"
                | "namespace_definition"
                | "template_declaration"
        ),
        SyntaxKind::ObjectiveC => false,
        SyntaxKind::WebText | SyntaxKind::Text => false,
    }
}

fn compact_text_context(path: &Path, source: &str) -> String {
    match extension(path).as_deref() {
        Some("m" | "mm") => compact_objective_c_context(source, 180),
        Some("md") => compact_markdown_context(source),
        Some("json" | "yaml" | "yml" | "toml") => compact_config_lines(path, source, 180),
        Some("vue" | "svelte" | "astro" | "html") => compact_web_context(source, 160),
        _ => compact_non_empty_lines(source, 160),
    }
}

fn build_text_tree_map(path: &Path, source: &str) -> String {
    match extension(path).as_deref() {
        Some("m" | "mm") => compact_objective_c_context(source, 120),
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
        Some("json" | "yaml" | "yml" | "toml") => compact_config_lines(path, source, 100),
        Some("vue" | "svelte" | "astro" | "html") => compact_web_context(source, 80),
        _ => compact_non_empty_lines(source, 80),
    }
}

fn compact_objective_c_context(source: &str, max_lines: usize) -> String {
    let mut output = Vec::new();
    let mut brace_depth = 0i32;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_objective_c_noise(trimmed) {
            continue;
        }

        if brace_depth > 0 {
            brace_depth = (brace_depth + brace_delta(trimmed)).max(0);
            continue;
        }

        let kept = if is_objective_c_header_line(trimmed)
            || is_objective_c_declaration(trimmed)
            || is_cpp_shape_line(trimmed)
        {
            Some(collapse_body_line(trimmed))
        } else {
            None
        };

        if let Some(line) = kept {
            output.push(truncate_line(line, 240));
            if output.len() >= max_lines {
                break;
            }
        }

        brace_depth = (brace_depth + brace_delta(trimmed)).max(0);
    }

    dedupe_preserving_order(output).join("\n")
}

fn is_objective_c_noise(line: &str) -> bool {
    line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
}

fn is_objective_c_header_line(line: &str) -> bool {
    line.starts_with("#import")
        || line.starts_with("#include")
        || line.starts_with("#define")
        || line.starts_with("#pragma")
        || line.starts_with("@class")
        || line.starts_with("@protocol")
        || line.starts_with("@interface")
        || line.starts_with("@implementation")
        || line.starts_with("@end")
        || line.starts_with("@property")
        || line.starts_with("@synthesize")
        || line.starts_with("@dynamic")
}

fn is_objective_c_declaration(line: &str) -> bool {
    (line.starts_with("- ")
        || line.starts_with("+ ")
        || line.starts_with("-(")
        || line.starts_with("+("))
        && line.contains(')')
}

fn is_cpp_shape_line(line: &str) -> bool {
    let first_word = line
        .split(|ch: char| ch.is_whitespace() || ch == '<' || ch == ':')
        .next()
        .unwrap_or_default();

    matches!(
        first_word,
        "class"
            | "struct"
            | "enum"
            | "namespace"
            | "template"
            | "typedef"
            | "using"
            | "extern"
            | "static"
            | "const"
            | "void"
            | "int"
            | "bool"
            | "float"
            | "double"
            | "std"
            | "NSString"
            | "NSArray"
            | "NSDictionary"
            | "instancetype"
    ) && (line.contains('(') || line.ends_with(';') || line.ends_with('{'))
        && !is_control_flow_line(line)
}

fn is_control_flow_line(line: &str) -> bool {
    matches!(
        line.split_whitespace().next().unwrap_or_default(),
        "if" | "for" | "while" | "switch" | "return" | "else" | "do"
    )
}

fn collapse_body_line(line: &str) -> String {
    if let Some((head, _)) = line.split_once('{') {
        let head = head.trim_end();
        if head.is_empty() {
            "{ ... }".to_owned()
        } else {
            format!("{head} {{ ... }}")
        }
    } else {
        line.to_owned()
    }
}

fn brace_delta(line: &str) -> i32 {
    line.chars().fold(0, |delta, ch| match ch {
        '{' => delta + 1,
        '}' => delta - 1,
        _ => delta,
    })
}

fn compact_web_context(source: &str, max_lines: usize) -> String {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("<!--") && !line.starts_with("//"))
        .map(|line| truncate_line(line.to_owned(), 240))
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_markdown_context(source: &str) -> String {
    let mut output = Vec::new();
    let mut keep_after_heading = 0usize;
    let mut in_kept_fence = false;
    let mut kept_fence_lines = 0usize;
    let mut in_dropped_fence = false;
    let mut table_buffer: Vec<String> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush_markdown_table(&mut output, &mut table_buffer);
            if in_kept_fence {
                output.push(trimmed.to_owned());
                in_kept_fence = false;
                continue;
            }
            if in_dropped_fence {
                output.push(trimmed.to_owned());
                in_dropped_fence = false;
                continue;
            }

            if is_important_fence(trimmed) {
                output.push(trimmed.to_owned());
                in_kept_fence = true;
                kept_fence_lines = 0;
            } else {
                output.push(trimmed.to_owned());
                output.push("...".to_owned());
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
            flush_markdown_table(&mut output, &mut table_buffer);
            continue;
        }

        if is_markdown_table_line(trimmed) {
            table_buffer.push(truncate_line(trimmed.to_owned(), 240));
            continue;
        }
        flush_markdown_table(&mut output, &mut table_buffer);

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
            continue;
        }

        if has_markdown_link(trimmed) {
            output.push(truncate_line(trimmed.to_owned(), 240));
        }
    }
    flush_markdown_table(&mut output, &mut table_buffer);

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

fn flush_markdown_table(output: &mut Vec<String>, table: &mut Vec<String>) {
    if table.is_empty() {
        return;
    }

    output.extend(sample_markdown_table(table));
    table.clear();
}

fn sample_markdown_table(table: &[String]) -> Vec<String> {
    if table.len() <= 14 {
        return table.to_vec();
    }

    let mut sampled = Vec::new();
    sampled.extend(table.iter().take(10).cloned());
    sampled.push("...".to_owned());

    let tail_start = table.len().saturating_sub(4);
    for row in &table[tail_start..] {
        if sampled.last() != Some(row) {
            sampled.push(row.clone());
        }
    }

    sampled
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
        || (is_markdown_table_separator(line) && !line.contains('|'))
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

fn is_markdown_table_line(line: &str) -> bool {
    line.starts_with('|') && line.ends_with('|') && line.matches('|').count() >= 2
}

fn is_markdown_table_separator(line: &str) -> bool {
    is_markdown_table_line(line) && line.chars().all(|ch| matches!(ch, '|' | '-' | ':' | ' '))
}

fn has_markdown_link(line: &str) -> bool {
    let has_inline_link = line.contains("](") && line.contains('[');
    let has_reference_link = line.starts_with('[') && line.contains("]:");
    has_inline_link || has_reference_link
}

fn compact_config_lines(path: &Path, source: &str, max_lines: usize) -> String {
    let mut output = Vec::new();
    let mut kept_array_items = 0usize;
    let mut collapsed_array = false;
    let mut sections: Vec<ConfigSection> = Vec::new();

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if is_top_level_config_comment(path, raw_line) {
            output.push(truncate_line(line.to_owned(), 240));
            if output.len() >= max_lines {
                break;
            }
            continue;
        }

        if is_noisy_config_line(line) {
            continue;
        }

        let indent = leading_whitespace(raw_line);
        close_config_sections(path, line, indent, &mut sections);
        let parent_important = sections.last().is_some_and(|section| section.important);
        let in_unimportant_section = sections.last().is_some_and(|section| !section.important);
        let line_key = config_line_key(path, line);
        let line_important = line_key.as_deref().is_some_and(is_important_config_key)
            || is_important_config_line(line);
        let keep_nested = parent_important || line_important;

        if is_array_item(line) {
            kept_array_items += 1;
            if !keep_nested {
                if !collapsed_array {
                    output.push("...".to_owned());
                    collapsed_array = true;
                }
                continue;
            }
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

        let top_level = is_top_level_config_line(line, indent)
            && !in_unimportant_section
            && (!is_toml_table_line(line) || keep_nested);
        if top_level || keep_nested || kept_array_items > 0 {
            output.push(truncate_line(line.to_owned(), 240));
        }

        open_config_section(path, line, indent, keep_nested, &mut sections);

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

#[derive(Debug, Clone, Copy)]
struct ConfigSection {
    indent: usize,
    important: bool,
    toml_table: bool,
}

fn leading_whitespace(line: &str) -> usize {
    line.chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .map(|ch| if ch == '\t' { 2 } else { 1 })
        .sum()
}

fn close_config_sections(
    path: &Path,
    line: &str,
    indent: usize,
    sections: &mut Vec<ConfigSection>,
) {
    if matches!(extension(path).as_deref(), Some("toml")) {
        if is_toml_table_line(line) {
            while sections.last().is_some_and(|section| section.toml_table) {
                sections.pop();
            }
        }
        return;
    }

    if line.starts_with('}') || line.starts_with(']') {
        sections.pop();
        return;
    }

    while sections
        .last()
        .is_some_and(|section| indent <= section.indent)
    {
        sections.pop();
    }
}

fn open_config_section(
    path: &Path,
    line: &str,
    indent: usize,
    important: bool,
    sections: &mut Vec<ConfigSection>,
) {
    let opens_structural_block = line.ends_with('{') || line.ends_with('[') || line.ends_with(":");
    let opens_toml_table =
        matches!(extension(path).as_deref(), Some("toml")) && is_toml_table_line(line);

    if opens_structural_block || opens_toml_table {
        sections.push(ConfigSection {
            indent,
            important,
            toml_table: opens_toml_table,
        });
    }
}

fn is_toml_table_line(line: &str) -> bool {
    line.starts_with('[') && line.ends_with(']')
}

fn is_noisy_config_line(line: &str) -> bool {
    line.is_empty()
        || line.starts_with('#')
        || line.starts_with("//")
        || matches!(line, "{" | "}" | "[" | "]" | "," | "}," | "],")
}

fn is_top_level_config_comment(path: &Path, raw_line: &str) -> bool {
    if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
        return false;
    }

    let line = raw_line.trim();
    match extension(path).as_deref() {
        Some("yaml" | "yml" | "toml") => line.starts_with('#'),
        Some("json") => line.starts_with("//"),
        _ => false,
    }
}

fn is_array_item(line: &str) -> bool {
    (line.starts_with('"') && !line.contains(':'))
        || line.starts_with('{')
        || line.starts_with('-')
        || line.starts_with("[[")
        || (line.ends_with(',') && !line.contains(':') && !line.contains('='))
}

fn is_top_level_config_line(line: &str, indent: usize) -> bool {
    line.starts_with('[')
        || line.starts_with("[[")
        || (indent <= 2 && line.starts_with('"'))
        || (!line.starts_with('-') && indent == 0 && (line.contains(':') || line.contains('=')))
}

fn is_important_config_line(line: &str) -> bool {
    config_line_key_from_trimmed(line).is_some_and(|key| is_important_config_key(&key))
}

const IMPORTANT_CONFIG_KEYS: &[&str] = &[
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
    "env",
    "permissions",
    "services",
    "matrix",
];

fn is_important_config_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    IMPORTANT_CONFIG_KEYS.iter().any(|important| {
        lower == *important
            || lower.ends_with(&format!(".{important}"))
            || lower.ends_with(&format!("-{important}"))
    })
}

fn config_line_key(path: &Path, line: &str) -> Option<String> {
    match extension(path).as_deref() {
        Some("toml") if line.starts_with('[') && line.ends_with(']') => Some(
            line.trim_matches(|ch| ch == '[' || ch == ']')
                .trim()
                .to_ascii_lowercase(),
        ),
        _ => config_line_key_from_trimmed(line),
    }
}

fn config_line_key_from_trimmed(line: &str) -> Option<String> {
    if let Some(after_quote) = line.strip_prefix('"') {
        let key = after_quote.split('"').next()?;
        return Some(key.to_ascii_lowercase());
    }

    if let Some((key, _)) = line.split_once(':') {
        return Some(key.trim().trim_matches('"').to_ascii_lowercase());
    }

    if let Some((key, _)) = line.split_once('=') {
        return Some(key.trim().trim_matches('"').to_ascii_lowercase());
    }

    None
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
            (
                "c",
                r#"
#include <stdio.h>

int greet(const char *name) {
    return printf("hello %s", name);
}
"#,
                "int greet",
                "return printf",
            ),
            (
                "h",
                r#"
#pragma once

int greet(const char *name);
"#,
                "int greet",
                "",
            ),
            (
                "cpp",
                r#"
#include <string>

class Greeter {
public:
    std::string greet(const std::string &name) {
        return "hello " + name;
    }
};
"#,
                "class Greeter",
                "return \"hello",
            ),
            (
                "hpp",
                r#"
#pragma once

class Greeter {
public:
    std::string greet(const std::string &name);
};
"#,
                "class Greeter",
                "",
            ),
            (
                "m",
                r#"
#import <Foundation/Foundation.h>

@implementation Greeter
- (NSString *)greet:(NSString *)name {
    return [@"hello " stringByAppendingString:name];
}
@end
"#,
                "greet:(NSString *)name",
                "",
            ),
            (
                "mm",
                r#"
#import <Foundation/Foundation.h>
#include <string>

@implementation Greeter
- (NSString *)greet:(NSString *)name {
    std::string prefix = "hello ";
    return [NSString stringWithUTF8String:prefix.c_str()];
}
@end
"#,
                "greet:(NSString *)name",
                "",
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
                body_text.is_empty() || !variants.skeleton.contains(body_text),
                "body kept for {extension}: {}",
                variants.skeleton
            );
        }
    }

    #[test]
    fn objective_c_compaction_keeps_structure() {
        let path = write_temp_source(
            "m",
            r#"
#import <Foundation/Foundation.h>

@interface Greeter : NSObject
@property (nonatomic, copy) NSString *prefix;
- (NSString *)greet:(NSString *)name;
@end

static NSString *FormatName(NSString *name) {
    return [name uppercaseString];
}

@implementation Greeter
- (NSString *)greet:(NSString *)name {
    NSString *formatted = FormatName(name);
    return [self.prefix stringByAppendingString:formatted];
}
@end
"#,
        );

        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants
            .skeleton
            .contains("#import <Foundation/Foundation.h>"));
        assert!(variants
            .skeleton
            .contains("@property (nonatomic, copy) NSString *prefix;"));
        assert!(variants
            .skeleton
            .contains("- (NSString *)greet:(NSString *)name { ... }"));
        assert!(variants
            .tree_map
            .contains("static NSString *FormatName(NSString *name) { ... }"));
        assert!(!variants.skeleton.contains("uppercaseString"));
        assert!(!variants.skeleton.contains("stringByAppendingString"));
    }

    #[test]
    fn objective_cpp_compaction_keeps_cpp_shapes() {
        let path = write_temp_source(
            "mm",
            r#"
#import <Foundation/Foundation.h>
#include <string>

class NameBuilder {
public:
    std::string build() const {
        return "hello";
    }
};

std::string BuildName() {
    return "world";
}

@implementation Greeter
- (NSString *)greet:(NSString *)name {
    std::string value = BuildName();
    return [NSString stringWithUTF8String:value.c_str()];
}
@end
"#,
        );

        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants.tree_map.contains("#include <string>"));
        assert!(variants.tree_map.contains("class NameBuilder { ... }"));
        assert!(variants
            .tree_map
            .contains("std::string BuildName() { ... }"));
        assert!(variants
            .tree_map
            .contains("- (NSString *)greet:(NSString *)name { ... }"));
        assert!(!variants.skeleton.contains("return \"world\""));
        assert!(!variants.skeleton.contains("value.c_str"));
    }

    #[test]
    fn web_templates_keep_compact_shape() {
        let cases = [
            (
                "vue",
                r#"
<script setup lang="ts">
const title = 'Hello'
</script>

<template>
  <main class="page">
    <h1>{{ title }}</h1>
  </main>
</template>
"#,
                "<script setup lang=\"ts\">",
                "<h1>{{ title }}</h1>",
            ),
            (
                "svelte",
                r#"
<script lang="ts">
  export let title: string
</script>

<main>
  <h1>{title}</h1>
</main>
"#,
                "<script lang=\"ts\">",
                "<h1>{title}</h1>",
            ),
            (
                "astro",
                r#"
---
const title = 'Hello'
---

<html>
  <body><h1>{title}</h1></body>
</html>
"#,
                "const title = 'Hello'",
                "<body><h1>{title}</h1></body>",
            ),
            (
                "html",
                r#"
<!doctype html>
<html>
  <body>
    <h1>Hello</h1>
  </body>
</html>
"#,
                "<!doctype html>",
                "<h1>Hello</h1>",
            ),
        ];

        for (extension, source, first_shape, second_shape) in cases {
            let path = write_temp_source(extension, source);
            let variants = compress_file(&path, CompressionLevel::TreeMap).unwrap();

            assert!(
                variants.tree_map.contains(first_shape),
                "missing first shape for {extension}: {}",
                variants.tree_map
            );
            assert!(
                variants.tree_map.contains(second_shape),
                "missing second shape for {extension}: {}",
                variants.tree_map
            );
        }
    }

    #[test]
    fn markdown_keeps_headings_summary_and_important_fences() {
        let path = write_temp_source(
            "md",
            r#"
<p align="center"><img src="badge.png" /></p>
| command | purpose |
| ----- | ----- |
| `bonsai .` | build context |

# Bonsai

Useful summary text.
More summary.

See the [schema](docs/output-schema.md).

[reference]: https://example.com/reference

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
        assert!(variants.skeleton.contains("| command | purpose |"));
        assert!(variants.skeleton.contains("| `bonsai .` | build context |"));
        assert!(variants
            .skeleton
            .contains("[schema](docs/output-schema.md)"));
        assert!(variants
            .skeleton
            .contains("[reference]: https://example.com/reference"));
        assert!(variants.skeleton.contains("```sh"));
        assert!(variants.skeleton.contains("bonsai ."));
        assert!(variants.skeleton.contains("```mermaid"));
        assert!(!variants.skeleton.contains("<img"));
        assert!(!variants.skeleton.contains("graph TD"));
    }

    #[test]
    fn markdown_samples_long_tables_with_header_and_tail() {
        let mut source = String::from("# Matrix\n\n| name | value |\n| --- | --- |\n");
        for index in 1..=20 {
            source.push_str(&format!("| row-{index} | value-{index} |\n"));
        }

        let path = write_temp_source("md", &source);
        let variants = compress_file(&path, CompressionLevel::Skeleton).unwrap();

        assert!(variants.skeleton.contains("| name | value |"));
        assert!(variants.skeleton.contains("| --- | --- |"));
        assert!(variants.skeleton.contains("| row-1 | value-1 |"));
        assert!(variants.skeleton.contains("| row-8 | value-8 |"));
        assert!(variants.skeleton.contains("..."));
        assert!(variants.skeleton.contains("| row-17 | value-17 |"));
        assert!(variants.skeleton.contains("| row-20 | value-20 |"));
        assert!(!variants.skeleton.contains("| row-12 | value-12 |"));
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
    fn config_keeps_top_level_comments_where_supported() {
        let yaml_path = write_temp_source(
            "yaml",
            r#"
# deployment defaults
name: demo
jobs:
  # internal job note
  build:
    runs-on: ubuntu-latest
"#,
        );
        let toml_path = write_temp_source(
            "toml",
            r#"
# package metadata
name = "demo"
[dependencies]
# nested-ish dependency note
serde = "1"
"#,
        );
        let json_path = write_temp_source(
            "json",
            r#"
// jsonc-style top note
{
  // nested note
  "name": "demo"
}
"#,
        );

        let yaml = compress_file(&yaml_path, CompressionLevel::Skeleton).unwrap();
        let toml = compress_file(&toml_path, CompressionLevel::Skeleton).unwrap();
        let json = compress_file(&json_path, CompressionLevel::Skeleton).unwrap();

        assert!(yaml.skeleton.contains("# deployment defaults"));
        assert!(!yaml.skeleton.contains("internal job note"));
        assert!(toml.skeleton.contains("# package metadata"));
        assert!(toml.skeleton.contains("# nested-ish dependency note"));
        assert!(json.skeleton.contains("// jsonc-style top note"));
        assert!(!json.skeleton.contains("nested note"));
    }

    #[test]
    fn config_keeps_nested_important_sections() {
        let json_path = write_temp_source(
            "json",
            r#"
{
  "name": "demo",
  "scripts": {
    "build": "vite build",
    "test": "cargo test"
  },
  "dependencies": {
    "serde": "1",
    "tokio": "1"
  },
  "fixtures": {
    "large": [
      "ignore-me"
    ]
  }
}
"#,
        );
        let yaml_path = write_temp_source(
            "yaml",
            r#"
name: demo
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
metadata:
  fixtures:
    - ignore-me
"#,
        );
        let toml_path = write_temp_source(
            "toml",
            r#"
name = "demo"

[workspace.dependencies]
serde = "1"
tokio = "1"

[package.metadata.fixtures]
large = "ignore-me"
"#,
        );

        let json = compress_file(&json_path, CompressionLevel::Skeleton).unwrap();
        let yaml = compress_file(&yaml_path, CompressionLevel::Skeleton).unwrap();
        let toml = compress_file(&toml_path, CompressionLevel::Skeleton).unwrap();

        assert!(json.skeleton.contains("\"build\": \"vite build\""));
        assert!(json.skeleton.contains("\"serde\": \"1\""));
        assert!(!json.skeleton.contains("ignore-me"));
        assert!(yaml.skeleton.contains("runs-on: ubuntu-latest"));
        assert!(yaml.skeleton.contains("- uses: actions/checkout@v4"));
        assert!(yaml.skeleton.contains("- run: cargo test"));
        assert!(!yaml.skeleton.contains("ignore-me"));
        assert!(toml.skeleton.contains("[workspace.dependencies]"));
        assert!(toml.skeleton.contains("serde = \"1\""));
        assert!(toml.skeleton.contains("tokio = \"1\""));
        assert!(!toml.skeleton.contains("ignore-me"));
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
