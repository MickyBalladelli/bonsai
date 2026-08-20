use std::collections::BTreeMap;

use crate::budget::ProcessedFile;

#[derive(Debug, Clone)]
pub struct DirectorySummary {
    pub path: String,
    pub file_count: usize,
    pub tokens: usize,
}

#[derive(Debug, Clone)]
pub struct FormatOptions {
    pub project_map_only: bool,
    pub project_map_mode: ProjectMapMode,
    pub include_file_hashes: bool,
    pub include_token_counts: bool,
    pub include_files: bool,
    pub include_content: bool,
    pub directory_summaries: Vec<DirectorySummary>,
    pub deleted_files: Vec<String>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            project_map_only: false,
            project_map_mode: ProjectMapMode::default(),
            include_file_hashes: false,
            include_token_counts: true,
            include_files: false,
            include_content: false,
            directory_summaries: Vec::new(),
            deleted_files: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProjectMapMode {
    #[default]
    Flat,
    Compact,
}

#[derive(Debug, Clone)]
pub struct RepositoryMetadata {
    pub generated_at: String,
    pub repo_root: String,
    pub max_tokens: usize,
    pub compression_level: u8,
    pub file_count: usize,
}

pub fn format_repository_context_xml(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
    options: &FormatOptions,
) -> String {
    if options.project_map_only {
        let mut output = String::new();
        push_project_map_xml(
            &mut output,
            files,
            options.include_file_hashes,
            options.include_token_counts,
            options.project_map_mode,
        );
        return output;
    }

    let mut output = String::new();
    output.push_str("<repository_context>\n");
    push_metadata_xml(&mut output, metadata);
    push_project_map_xml(
        &mut output,
        files,
        options.include_file_hashes,
        options.include_token_counts,
        options.project_map_mode,
    );
    push_deleted_files_xml(&mut output, &options.deleted_files);
    push_directory_summaries_xml(
        &mut output,
        &options.directory_summaries,
        options.include_token_counts,
    );

    if options.include_files {
        output.push_str("<files>\n");

        for file in files {
            output.push_str("<file path=\"");
            push_xml_escaped(&mut output, &file.path);
            output.push_str("\" level=\"");
            output.push_str(&file.level.as_u8().to_string());
            if options.include_token_counts {
                output.push_str("\" tokens=\"");
                output.push_str(&file.token_count.to_string());
            }
            if options.include_content {
                output.push_str("\">");
                push_xml_escaped(&mut output, file.content());
                output.push_str("</file>\n");
            } else {
                output.push_str("\" />\n");
            }
        }

        output.push_str("</files>\n");
    }

    output.push_str("</repository_context>\n");
    output
}

pub fn format_repository_context_json(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
    options: &FormatOptions,
) -> String {
    if options.project_map_only {
        let mut output = String::new();
        push_project_map_json(
            &mut output,
            files,
            0,
            options.include_file_hashes,
            options.include_token_counts,
            options.project_map_mode,
        );
        output.push('\n');
        return output;
    }

    let mut output = String::new();
    output.push_str("{\n");
    output.push_str("  \"metadata\": ");
    push_metadata_json(&mut output, metadata);
    output.push_str(",\n  \"project_map\": ");
    push_project_map_json(
        &mut output,
        files,
        2,
        options.include_file_hashes,
        options.include_token_counts,
        options.project_map_mode,
    );

    if !options.deleted_files.is_empty() {
        output.push_str(",\n  \"deleted_files\": ");
        push_deleted_files_json(&mut output, &options.deleted_files, 2);
    }

    if !options.directory_summaries.is_empty() {
        output.push_str(",\n  \"directory_summaries\": ");
        push_directory_summaries_json(
            &mut output,
            &options.directory_summaries,
            2,
            options.include_token_counts,
        );
    }

    if options.include_files {
        output.push_str(",\n  \"files\": [\n");

        for (index, file) in files.iter().enumerate() {
            if index > 0 {
                output.push_str(",\n");
            }
            output.push_str("    ");
            push_file_json(
                &mut output,
                file,
                options.include_content,
                options.include_token_counts,
            );
        }

        output.push_str("\n  ]");
    }

    output.push_str("\n}\n");
    output
}

pub fn format_repository_context_text(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
    options: &FormatOptions,
) -> String {
    if options.project_map_only {
        let mut output = String::new();
        push_project_map_text(
            &mut output,
            files,
            options.include_file_hashes,
            options.include_token_counts,
            options.project_map_mode,
        );
        return output;
    }

    let mut output = String::new();
    output.push_str("bonsai_context\n");
    output.push_str("generated_at: ");
    output.push_str(&metadata.generated_at);
    output.push_str("\nrepo_root: ");
    output.push_str(&metadata.repo_root);
    output.push_str("\nmax_tokens: ");
    output.push_str(&metadata.max_tokens.to_string());
    output.push_str("\ncompression_level: ");
    output.push_str(&metadata.compression_level.to_string());
    output.push_str("\nfile_count: ");
    output.push_str(&metadata.file_count.to_string());
    output.push_str("\n\n");

    push_project_map_text(
        &mut output,
        files,
        options.include_file_hashes,
        options.include_token_counts,
        options.project_map_mode,
    );
    push_deleted_files_text(&mut output, &options.deleted_files);
    push_directory_summaries_text(
        &mut output,
        &options.directory_summaries,
        options.include_token_counts,
    );

    if options.include_files {
        output.push_str("files\n");
        for file in files {
            output.push_str("--- ");
            output.push_str(&file.path);
            output.push_str(" L");
            output.push_str(&file.level.as_u8().to_string());
            if options.include_token_counts {
                output.push_str(" tokens=");
                output.push_str(&file.token_count.to_string());
            }
            output.push('\n');
            if options.include_content {
                output.push_str(file.content());
                if !file.content().ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    output
}

fn push_metadata_xml(output: &mut String, metadata: &RepositoryMetadata) {
    output.push_str("<metadata generated_at=\"");
    push_xml_escaped(output, &metadata.generated_at);
    output.push_str("\" repo_root=\"");
    push_xml_escaped(output, &metadata.repo_root);
    output.push_str("\" max_tokens=\"");
    output.push_str(&metadata.max_tokens.to_string());
    output.push_str("\" compression_level=\"");
    output.push_str(&metadata.compression_level.to_string());
    output.push_str("\" file_count=\"");
    output.push_str(&metadata.file_count.to_string());
    output.push_str("\" />\n");
}

fn push_project_map_xml(
    output: &mut String,
    files: &[ProcessedFile],
    include_hash: bool,
    include_tokens: bool,
    mode: ProjectMapMode,
) {
    if mode == ProjectMapMode::Compact {
        push_compact_project_map_xml(output, files, include_hash, include_tokens);
        return;
    }

    output.push_str("<project_map>\n");
    for file in files {
        output.push_str("<entry path=\"");
        push_xml_escaped(output, &file.path);
        output.push_str("\" level=\"");
        output.push_str(&file.level.as_u8().to_string());
        if include_tokens {
            output.push_str("\" tokens=\"");
            output.push_str(&file.token_count.to_string());
        }
        if include_hash {
            if let Some(hash) = &file.content_hash {
                output.push_str("\" hash=\"");
                push_xml_escaped(output, hash);
            }
        }
        output.push_str("\" />\n");
    }
    output.push_str("</project_map>\n");
}

fn push_compact_project_map_xml(
    output: &mut String,
    files: &[ProcessedFile],
    include_hash: bool,
    include_tokens: bool,
) {
    output.push_str("<project_map mode=\"compact\">\n");
    for directory in project_map_directories(files) {
        output.push_str("<dir path=\"");
        push_xml_escaped(output, &directory.path);
        output.push_str("\" files=\"");
        output.push_str(&directory.file_count.to_string());
        if include_tokens {
            output.push_str("\" tokens=\"");
            output.push_str(&directory.tokens.to_string());
        }
        output.push_str("\">\n");
        for file in directory.files {
            output.push_str("<entry name=\"");
            push_xml_escaped(output, file_name(&file.path));
            output.push_str("\" level=\"");
            output.push_str(&file.level.as_u8().to_string());
            if include_tokens {
                output.push_str("\" tokens=\"");
                output.push_str(&file.token_count.to_string());
            }
            if include_hash {
                if let Some(hash) = &file.content_hash {
                    output.push_str("\" hash=\"");
                    push_xml_escaped(output, hash);
                }
            }
            output.push_str("\" />\n");
        }
        output.push_str("</dir>\n");
    }
    output.push_str("</project_map>\n");
}

fn push_directory_summaries_xml(
    output: &mut String,
    summaries: &[DirectorySummary],
    include_tokens: bool,
) {
    if summaries.is_empty() {
        return;
    }

    output.push_str("<directory_summaries>\n");
    for summary in summaries {
        output.push_str("<directory path=\"");
        push_xml_escaped(output, &summary.path);
        output.push_str("\" files=\"");
        output.push_str(&summary.file_count.to_string());
        if include_tokens {
            output.push_str("\" tokens=\"");
            output.push_str(&summary.tokens.to_string());
        }
        output.push_str("\" />\n");
    }
    output.push_str("</directory_summaries>\n");
}

fn push_deleted_files_xml(output: &mut String, deleted_files: &[String]) {
    if deleted_files.is_empty() {
        return;
    }

    output.push_str("<deleted_files>\n");
    for path in deleted_files {
        output.push_str("<deleted path=\"");
        push_xml_escaped(output, path);
        output.push_str("\" />\n");
    }
    output.push_str("</deleted_files>\n");
}

fn push_metadata_json(output: &mut String, metadata: &RepositoryMetadata) {
    output.push_str("{");
    output.push_str("\"generated_at\":\"");
    push_json_escaped(output, &metadata.generated_at);
    output.push_str("\",\"repo_root\":\"");
    push_json_escaped(output, &metadata.repo_root);
    output.push_str("\",\"max_tokens\":");
    output.push_str(&metadata.max_tokens.to_string());
    output.push_str(",\"compression_level\":");
    output.push_str(&metadata.compression_level.to_string());
    output.push_str(",\"file_count\":");
    output.push_str(&metadata.file_count.to_string());
    output.push_str("}");
}

fn push_deleted_files_json(output: &mut String, deleted_files: &[String], indent: usize) {
    let base = " ".repeat(indent);
    let item = " ".repeat(indent + 2);
    output.push_str("[\n");

    for (index, path) in deleted_files.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str(&item);
        output.push_str("{\"path\":\"");
        push_json_escaped(output, path);
        output.push_str("\"}");
    }

    output.push('\n');
    output.push_str(&base);
    output.push(']');
}

fn push_project_map_text(
    output: &mut String,
    files: &[ProcessedFile],
    include_hash: bool,
    include_tokens: bool,
    mode: ProjectMapMode,
) {
    if mode == ProjectMapMode::Compact {
        push_compact_project_map_text(output, files, include_hash, include_tokens);
        return;
    }

    output.push_str("project_map\n");
    for file in files {
        output.push_str(&file.path);
        output.push_str(" L");
        output.push_str(&file.level.as_u8().to_string());
        if include_tokens {
            output.push_str(" tokens=");
            output.push_str(&file.token_count.to_string());
        }
        if include_hash {
            if let Some(hash) = &file.content_hash {
                output.push_str(" hash=");
                output.push_str(hash);
            }
        }
        output.push('\n');
    }
    output.push('\n');
}

fn push_compact_project_map_text(
    output: &mut String,
    files: &[ProcessedFile],
    include_hash: bool,
    include_tokens: bool,
) {
    output.push_str("project_map compact\n");
    for directory in project_map_directories(files) {
        output.push('[');
        output.push_str(&directory.path);
        output.push_str("] files=");
        output.push_str(&directory.file_count.to_string());
        if include_tokens {
            output.push_str(" tokens=");
            output.push_str(&directory.tokens.to_string());
        }
        output.push('\n');
        for file in directory.files {
            output.push_str(file_name(&file.path));
            output.push_str(" L");
            output.push_str(&file.level.as_u8().to_string());
            if include_tokens {
                output.push_str(" tokens=");
                output.push_str(&file.token_count.to_string());
            }
            if include_hash {
                if let Some(hash) = &file.content_hash {
                    output.push_str(" hash=");
                    output.push_str(hash);
                }
            }
            output.push('\n');
        }
    }
    output.push('\n');
}

fn push_deleted_files_text(output: &mut String, deleted_files: &[String]) {
    if deleted_files.is_empty() {
        return;
    }

    output.push_str("deleted_files\n");
    for path in deleted_files {
        output.push_str(path);
        output.push('\n');
    }
    output.push('\n');
}

fn push_directory_summaries_text(
    output: &mut String,
    summaries: &[DirectorySummary],
    include_tokens: bool,
) {
    if summaries.is_empty() {
        return;
    }

    output.push_str("directory_summaries\n");
    for summary in summaries {
        output.push_str(&summary.path);
        output.push_str(" files=");
        output.push_str(&summary.file_count.to_string());
        if include_tokens {
            output.push_str(" tokens=");
            output.push_str(&summary.tokens.to_string());
        }
        output.push('\n');
    }
    output.push('\n');
}

fn push_project_map_json(
    output: &mut String,
    files: &[ProcessedFile],
    indent: usize,
    include_hash: bool,
    include_tokens: bool,
    mode: ProjectMapMode,
) {
    if mode == ProjectMapMode::Compact {
        push_compact_project_map_json(output, files, indent, include_hash, include_tokens);
        return;
    }

    let base = " ".repeat(indent);
    let item = " ".repeat(indent + 2);
    output.push_str("[\n");

    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str(&item);
        push_file_map_json(output, file, include_hash, include_tokens);
    }

    output.push('\n');
    output.push_str(&base);
    output.push(']');
}

fn push_compact_project_map_json(
    output: &mut String,
    files: &[ProcessedFile],
    indent: usize,
    include_hash: bool,
    include_tokens: bool,
) {
    let base = " ".repeat(indent);
    let item = " ".repeat(indent + 2);
    output.push_str("[\n");

    for (index, directory) in project_map_directories(files).into_iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str(&item);
        output.push_str("{\"path\":\"");
        push_json_escaped(output, &directory.path);
        output.push_str("\",\"files\":");
        output.push_str(&directory.file_count.to_string());
        if include_tokens {
            output.push_str(",\"tokens\":");
            output.push_str(&directory.tokens.to_string());
        }
        output.push_str(",\"entries\":[");

        for (file_index, file) in directory.files.iter().enumerate() {
            if file_index > 0 {
                output.push(',');
            }
            push_compact_file_map_json(output, file, include_hash, include_tokens);
        }

        output.push_str("]}");
    }

    output.push('\n');
    output.push_str(&base);
    output.push(']');
}

fn push_compact_file_map_json(
    output: &mut String,
    file: &ProcessedFile,
    include_hash: bool,
    include_tokens: bool,
) {
    output.push_str("{\"name\":\"");
    push_json_escaped(output, file_name(&file.path));
    output.push_str("\",\"level\":");
    output.push_str(&file.level.as_u8().to_string());
    if include_tokens {
        output.push_str(",\"tokens\":");
        output.push_str(&file.token_count.to_string());
    }
    if include_hash {
        if let Some(hash) = &file.content_hash {
            output.push_str(",\"hash\":\"");
            push_json_escaped(output, hash);
            output.push('"');
        }
    }
    output.push('}');
}

#[derive(Debug)]
struct ProjectMapDirectory<'a> {
    path: String,
    file_count: usize,
    tokens: usize,
    files: Vec<&'a ProcessedFile>,
}

fn project_map_directories(files: &[ProcessedFile]) -> Vec<ProjectMapDirectory<'_>> {
    let mut by_dir: BTreeMap<String, Vec<&ProcessedFile>> = BTreeMap::new();
    for file in files {
        by_dir
            .entry(directory_name(&file.path).to_owned())
            .or_default()
            .push(file);
    }

    by_dir
        .into_iter()
        .map(|(path, files)| ProjectMapDirectory {
            path,
            file_count: files.len(),
            tokens: files.iter().map(|file| file.token_count).sum(),
            files,
        })
        .collect()
}

fn directory_name(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or(".")
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map(|(_, name)| name).unwrap_or(path)
}

fn push_directory_summaries_json(
    output: &mut String,
    summaries: &[DirectorySummary],
    indent: usize,
    include_tokens: bool,
) {
    let base = " ".repeat(indent);
    let item = " ".repeat(indent + 2);
    output.push_str("[\n");

    for (index, summary) in summaries.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str(&item);
        output.push_str("{\"path\":\"");
        push_json_escaped(output, &summary.path);
        output.push_str("\",\"files\":");
        output.push_str(&summary.file_count.to_string());
        if include_tokens {
            output.push_str(",\"tokens\":");
            output.push_str(&summary.tokens.to_string());
        }
        output.push('}');
    }

    output.push('\n');
    output.push_str(&base);
    output.push(']');
}

fn push_file_map_json(
    output: &mut String,
    file: &ProcessedFile,
    include_hash: bool,
    include_tokens: bool,
) {
    output.push_str("{\"path\":\"");
    push_json_escaped(output, &file.path);
    output.push_str("\",\"level\":");
    output.push_str(&file.level.as_u8().to_string());
    if include_tokens {
        output.push_str(",\"tokens\":");
        output.push_str(&file.token_count.to_string());
    }
    if include_hash {
        if let Some(hash) = &file.content_hash {
            output.push_str(",\"hash\":\"");
            push_json_escaped(output, hash);
            output.push('"');
        }
    }
    output.push_str("}");
}

fn push_file_json(
    output: &mut String,
    file: &ProcessedFile,
    include_content: bool,
    include_tokens: bool,
) {
    output.push_str("{\"path\":\"");
    push_json_escaped(output, &file.path);
    output.push_str("\",\"level\":");
    output.push_str(&file.level.as_u8().to_string());
    if include_tokens {
        output.push_str(",\"tokens\":");
        output.push_str(&file.token_count.to_string());
    }
    if include_content {
        output.push_str(",\"content\":\"");
        push_json_escaped(output, file.content());
        output.push('"');
    }
    output.push('}');
}

fn push_xml_escaped(output: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(ch),
        }
    }
}

fn push_json_escaped(output: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch.is_control() => {
                output.push_str("\\u");
                output.push_str(&format!("{:04x}", ch as u32));
            }
            _ => output.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::ProcessedFile;
    use crate::parser::{CompressionLevel, FileVariants};

    #[test]
    fn xml_escapes_paths_and_content_with_metadata() {
        let files = vec![processed_file()];
        let xml = format_repository_context_xml(&files, &metadata(), &full_options());

        assert!(xml.contains("<metadata generated_at=\"1234567890\""));
        assert!(xml.contains("<project_map>"));
        assert!(xml.contains("path=\"src/&lt;bad&gt;&amp;&quot;name&quot;.rs\""));
        assert!(xml.contains("tokens=\"7\""));
        assert!(xml.contains("a &lt; b &amp;&amp; name == &apos;x&apos;"));
        assert!(xml.contains("{ &quot;yes&quot; }"));
    }

    #[test]
    fn json_escapes_content_and_includes_project_map() {
        let files = vec![processed_file()];
        let json = format_repository_context_json(&files, &metadata(), &full_options());

        assert!(json.contains("\"metadata\""));
        assert!(json.contains("\"project_map\""));
        assert!(json.contains("\"path\":\"src/<bad>&\\\"name\\\".rs\""));
        assert!(json.contains("\"tokens\":7"));
        assert!(json.contains("if a < b && name == 'x' { \\\"yes\\\" }"));
    }

    #[test]
    fn text_includes_low_overhead_context() {
        let files = vec![processed_file()];
        let text = format_repository_context_text(&files, &metadata(), &full_options());

        assert!(text.starts_with("bonsai_context\n"));
        assert!(text.contains("project_map\n"));
        assert!(text.contains("src/<bad>&\"name\".rs L2 tokens=7"));
        assert!(text.contains("files\n--- src/<bad>&\"name\".rs L2 tokens=7\n"));
        assert!(text.contains("if a < b && name == 'x' { \"yes\" }"));
        assert!(!text.contains("<repository_context>"));
        assert!(!text.contains("\"project_map\""));
    }

    #[test]
    fn can_omit_file_bodies() {
        let files = vec![processed_file()];
        let options = FormatOptions {
            include_files: false,
            include_content: false,
            ..FormatOptions::default()
        };

        let xml = format_repository_context_xml(&files, &metadata(), &options);
        let json = format_repository_context_json(&files, &metadata(), &options);

        assert!(xml.contains("<project_map>"));
        assert!(!xml.contains("<files>"));
        assert!(json.contains("\"project_map\""));
        assert!(!json.contains("\"files\""));
    }

    #[test]
    fn can_emit_project_map_only() {
        let files = vec![processed_file()];
        let options = FormatOptions {
            project_map_only: true,
            include_files: false,
            include_content: false,
            ..FormatOptions::default()
        };

        let xml = format_repository_context_xml(&files, &metadata(), &options);
        let json = format_repository_context_json(&files, &metadata(), &options);

        assert!(xml.starts_with("<project_map>"));
        assert!(!xml.contains("<metadata"));
        assert!(json.starts_with("["));
        assert!(!json.contains("\"metadata\""));
    }

    #[test]
    fn can_emit_compact_project_map() {
        let mut root = processed_file();
        root.path = "Cargo.toml".to_owned();
        root.token_count = 5;
        let mut nested = processed_file();
        nested.path = "src/lib.rs".to_owned();
        nested.token_count = 7;
        let files = vec![root, nested];
        let options = FormatOptions {
            project_map_mode: ProjectMapMode::Compact,
            include_files: false,
            include_content: false,
            ..FormatOptions::default()
        };

        let xml = format_repository_context_xml(&files, &metadata(), &options);
        let json = format_repository_context_json(&files, &metadata(), &options);

        assert!(xml.contains("<project_map mode=\"compact\">"));
        assert!(xml.contains("<dir path=\".\" files=\"1\" tokens=\"5\">"));
        assert!(xml.contains("<entry name=\"Cargo.toml\" level=\"2\" tokens=\"5\" />"));
        assert!(xml.contains("<dir path=\"src\" files=\"1\" tokens=\"7\">"));
        assert!(!xml.contains("path=\"src/lib.rs\" level=\"2\""));
        assert!(json.contains("\"path\":\"src\",\"files\":1,\"tokens\":7"));
        assert!(json.contains("\"entries\":[{\"name\":\"lib.rs\",\"level\":2,\"tokens\":7}]"));
    }

    #[test]
    fn can_omit_token_counts() {
        let mut root = processed_file();
        root.path = "Cargo.toml".to_owned();
        root.token_count = 5;
        let mut nested = processed_file();
        nested.path = "src/lib.rs".to_owned();
        nested.token_count = 7;
        let files = vec![root, nested];
        let options = FormatOptions {
            project_map_mode: ProjectMapMode::Compact,
            include_token_counts: false,
            include_files: true,
            include_content: true,
            directory_summaries: vec![DirectorySummary {
                path: "src".to_owned(),
                file_count: 1,
                tokens: 7,
            }],
            ..FormatOptions::default()
        };

        let xml = format_repository_context_xml(&files, &metadata(), &options);
        let json = format_repository_context_json(&files, &metadata(), &options);
        let text = format_repository_context_text(&files, &metadata(), &options);

        assert!(xml.contains("<dir path=\"src\" files=\"1\">"));
        assert!(xml.contains("<entry name=\"lib.rs\" level=\"2\" />"));
        assert!(xml.contains("<file path=\"src/lib.rs\" level=\"2\">"));
        assert!(!xml.contains(" tokens=\""));
        assert!(json.contains("\"path\":\"src\",\"files\":1,\"entries\""));
        assert!(json.contains("\"name\":\"lib.rs\",\"level\":2"));
        assert!(json.contains("\"path\":\"src/lib.rs\",\"level\":2,\"content\""));
        assert!(!json.contains("\"tokens\""));
        assert!(text.contains("[src] files=1"));
        assert!(text.contains("lib.rs L2\n"));
        assert!(!text.contains("tokens="));
    }

    #[test]
    fn includes_project_map_hashes_when_requested() {
        let mut file = processed_file();
        file.content_hash = Some("abc123".to_owned());
        let files = vec![file];
        let options = FormatOptions {
            include_file_hashes: true,
            include_files: false,
            include_content: false,
            ..FormatOptions::default()
        };

        let xml = format_repository_context_xml(&files, &metadata(), &options);
        let json = format_repository_context_json(&files, &metadata(), &options);

        assert!(xml.contains("hash=\"abc123\""));
        assert!(json.contains("\"hash\":\"abc123\""));
    }

    #[test]
    fn omits_project_map_hashes_by_default() {
        let mut file = processed_file();
        file.content_hash = Some("abc123".to_owned());
        let files = vec![file];

        let xml = format_repository_context_xml(&files, &metadata(), &full_options());
        let json = format_repository_context_json(&files, &metadata(), &full_options());

        assert!(!xml.contains("hash=\"abc123\""));
        assert!(!json.contains("\"hash\":\"abc123\""));
    }

    #[test]
    fn includes_directory_summaries() {
        let files = vec![processed_file()];
        let options = FormatOptions {
            include_files: true,
            include_content: true,
            directory_summaries: vec![DirectorySummary {
                path: "src".to_owned(),
                file_count: 2,
                tokens: 10,
            }],
            ..FormatOptions::default()
        };

        let xml = format_repository_context_xml(&files, &metadata(), &options);
        let json = format_repository_context_json(&files, &metadata(), &options);

        assert!(xml.contains("<directory_summaries>"));
        assert!(xml.contains("path=\"src\""));
        assert!(json.contains("\"directory_summaries\""));
        assert!(json.contains("\"path\":\"src\""));
    }

    #[test]
    fn includes_deleted_file_markers() {
        let files = vec![processed_file()];
        let options = FormatOptions {
            include_files: true,
            include_content: true,
            deleted_files: vec!["src/deleted.rs".to_owned()],
            ..FormatOptions::default()
        };

        let xml = format_repository_context_xml(&files, &metadata(), &options);
        let json = format_repository_context_json(&files, &metadata(), &options);

        assert!(xml.contains("<deleted_files>"));
        assert!(xml.contains("<deleted path=\"src/deleted.rs\" />"));
        assert!(json.contains("\"deleted_files\""));
        assert!(json.contains("\"path\":\"src/deleted.rs\""));
    }

    fn processed_file() -> ProcessedFile {
        let mut file = ProcessedFile::new(
            "src/<bad>&\"name\".rs".to_owned(),
            CompressionLevel::Skeleton,
            FileVariants {
                full: Some("ignored".to_owned()),
                skeleton: "if a < b && name == 'x' { \"yes\" }".to_owned(),
                tree_map: String::new(),
            },
        );
        file.token_count = 7;
        file
    }

    fn metadata() -> RepositoryMetadata {
        RepositoryMetadata {
            generated_at: "1234567890".to_owned(),
            repo_root: "/tmp/bonsai-context".to_owned(),
            max_tokens: 12000,
            compression_level: 2,
            file_count: 1,
        }
    }

    fn full_options() -> FormatOptions {
        FormatOptions {
            include_files: true,
            include_content: true,
            ..FormatOptions::default()
        }
    }
}
