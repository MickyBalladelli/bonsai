use crate::budget::ProcessedFile;

#[derive(Debug, Clone)]
pub struct DirectorySummary {
    pub path: String,
    pub file_count: usize,
    pub tokens: usize,
}

#[derive(Debug, Clone, Default)]
pub struct FormatOptions {
    pub project_map_only: bool,
    pub include_files: bool,
    pub include_content: bool,
    pub directory_summaries: Vec<DirectorySummary>,
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
        push_project_map_xml(&mut output, files);
        return output;
    }

    let mut output = String::new();
    output.push_str("<repository_context>\n");
    push_metadata_xml(&mut output, metadata);
    push_project_map_xml(&mut output, files);
    push_directory_summaries_xml(&mut output, &options.directory_summaries);

    if options.include_files {
        output.push_str("<files>\n");

        for file in files {
            output.push_str("<file path=\"");
            push_xml_escaped(&mut output, &file.path);
            output.push_str("\" level=\"");
            output.push_str(&file.level.as_u8().to_string());
            output.push_str("\" tokens=\"");
            output.push_str(&file.token_count.to_string());
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
        push_project_map_json(&mut output, files, 0);
        output.push('\n');
        return output;
    }

    let mut output = String::new();
    output.push_str("{\n");
    output.push_str("  \"metadata\": ");
    push_metadata_json(&mut output, metadata);
    output.push_str(",\n  \"project_map\": ");
    push_project_map_json(&mut output, files, 2);

    if !options.directory_summaries.is_empty() {
        output.push_str(",\n  \"directory_summaries\": ");
        push_directory_summaries_json(&mut output, &options.directory_summaries, 2);
    }

    if options.include_files {
        output.push_str(",\n  \"files\": [\n");

        for (index, file) in files.iter().enumerate() {
            if index > 0 {
                output.push_str(",\n");
            }
            output.push_str("    ");
            push_file_json(&mut output, file, options.include_content);
        }

        output.push_str("\n  ]");
    }

    output.push_str("\n}\n");
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

fn push_project_map_xml(output: &mut String, files: &[ProcessedFile]) {
    output.push_str("<project_map>\n");
    for file in files {
        output.push_str("<entry path=\"");
        push_xml_escaped(output, &file.path);
        output.push_str("\" level=\"");
        output.push_str(&file.level.as_u8().to_string());
        output.push_str("\" tokens=\"");
        output.push_str(&file.token_count.to_string());
        output.push_str("\" />\n");
    }
    output.push_str("</project_map>\n");
}

fn push_directory_summaries_xml(output: &mut String, summaries: &[DirectorySummary]) {
    if summaries.is_empty() {
        return;
    }

    output.push_str("<directory_summaries>\n");
    for summary in summaries {
        output.push_str("<directory path=\"");
        push_xml_escaped(output, &summary.path);
        output.push_str("\" files=\"");
        output.push_str(&summary.file_count.to_string());
        output.push_str("\" tokens=\"");
        output.push_str(&summary.tokens.to_string());
        output.push_str("\" />\n");
    }
    output.push_str("</directory_summaries>\n");
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

fn push_project_map_json(output: &mut String, files: &[ProcessedFile], indent: usize) {
    let base = " ".repeat(indent);
    let item = " ".repeat(indent + 2);
    output.push_str("[\n");

    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str(&item);
        push_file_map_json(output, file);
    }

    output.push('\n');
    output.push_str(&base);
    output.push(']');
}

fn push_directory_summaries_json(
    output: &mut String,
    summaries: &[DirectorySummary],
    indent: usize,
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
        output.push_str(",\"tokens\":");
        output.push_str(&summary.tokens.to_string());
        output.push('}');
    }

    output.push('\n');
    output.push_str(&base);
    output.push(']');
}

fn push_file_map_json(output: &mut String, file: &ProcessedFile) {
    output.push_str("{\"path\":\"");
    push_json_escaped(output, &file.path);
    output.push_str("\",\"level\":");
    output.push_str(&file.level.as_u8().to_string());
    output.push_str(",\"tokens\":");
    output.push_str(&file.token_count.to_string());
    output.push_str("}");
}

fn push_file_json(output: &mut String, file: &ProcessedFile, include_content: bool) {
    output.push_str("{\"path\":\"");
    push_json_escaped(output, &file.path);
    output.push_str("\",\"level\":");
    output.push_str(&file.level.as_u8().to_string());
    output.push_str(",\"tokens\":");
    output.push_str(&file.token_count.to_string());
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
