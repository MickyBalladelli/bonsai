use crate::budget::ProcessedFile;

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
) -> String {
    let mut output = String::new();
    output.push_str("<repository_context>\n");
    push_metadata_xml(&mut output, metadata);
    push_project_map_xml(&mut output, files);
    output.push_str("<files>\n");

    for file in files {
        output.push_str("<file path=\"");
        push_xml_escaped(&mut output, &file.path);
        output.push_str("\" level=\"");
        output.push_str(&file.level.as_u8().to_string());
        output.push_str("\" tokens=\"");
        output.push_str(&file.token_count.to_string());
        output.push_str("\">");
        push_xml_escaped(&mut output, file.content());
        output.push_str("</file>\n");
    }

    output.push_str("</files>\n");
    output.push_str("</repository_context>\n");
    output
}

pub fn format_repository_context_json(
    files: &[ProcessedFile],
    metadata: &RepositoryMetadata,
) -> String {
    let mut output = String::new();
    output.push_str("{\n");
    output.push_str("  \"metadata\": ");
    push_metadata_json(&mut output, metadata);
    output.push_str(",\n  \"project_map\": [\n");

    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str("    ");
        push_file_map_json(&mut output, file);
    }

    output.push_str("\n  ],\n  \"files\": [\n");

    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            output.push_str(",\n");
        }
        output.push_str("    ");
        push_file_json(&mut output, file);
    }

    output.push_str("\n  ]\n");
    output.push_str("}\n");
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

fn push_file_map_json(output: &mut String, file: &ProcessedFile) {
    output.push_str("{\"path\":\"");
    push_json_escaped(output, &file.path);
    output.push_str("\",\"level\":");
    output.push_str(&file.level.as_u8().to_string());
    output.push_str(",\"tokens\":");
    output.push_str(&file.token_count.to_string());
    output.push_str("}");
}

fn push_file_json(output: &mut String, file: &ProcessedFile) {
    output.push_str("{\"path\":\"");
    push_json_escaped(output, &file.path);
    output.push_str("\",\"level\":");
    output.push_str(&file.level.as_u8().to_string());
    output.push_str(",\"tokens\":");
    output.push_str(&file.token_count.to_string());
    output.push_str(",\"content\":\"");
    push_json_escaped(output, file.content());
    output.push_str("\"}");
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
        let xml = format_repository_context_xml(&files, &metadata());

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
        let json = format_repository_context_json(&files, &metadata());

        assert!(json.contains("\"metadata\""));
        assert!(json.contains("\"project_map\""));
        assert!(json.contains("\"path\":\"src/<bad>&\\\"name\\\".rs\""));
        assert!(json.contains("\"tokens\":7"));
        assert!(json.contains("if a < b && name == 'x' { \\\"yes\\\" }"));
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
}
