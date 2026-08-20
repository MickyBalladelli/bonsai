# Bonsai Output Schema

Bonsai writes XML by default. JSON is available with `--format json`. Lower-overhead text is available with `--format text`.

## XML

Default XML shape:

```xml
<repository_context>
  <metadata generated_at="unix_seconds" repo_root="/path/to/repo" max_tokens="12000" compression_level="2" file_count="3" />
  <project_map>
    <entry path="src/main.rs" level="2" tokens="120" />
  </project_map>
  <deleted_files>
    <deleted path="src/old.rs" />
  </deleted_files>
  <directory_summaries>
    <directory path="src" files="2" tokens="240" />
  </directory_summaries>
  <files>
    <file path="src/main.rs" level="2" tokens="120">compressed content</file>
  </files>
</repository_context>
```

`directory_summaries` appears only with `--directory-summaries`.

`deleted_files` appears when incremental output detects files that were present in the baseline but are now gone.

`--no-content` omits `files`.

`--no-token-counts` omits `tokens` attributes from `project_map`, `directory_summaries`, and `files`.

`--changed-since <git-ref>` uses `git diff` and emits added/changed files plus deleted file markers.

`--project-map-only` emits only:

```xml
<project_map>
  <entry path="src/main.rs" level="2" tokens="120" />
</project_map>
```

`--project-map compact` groups entries by directory:

```xml
<project_map mode="compact">
  <dir path="src" files="2" tokens="240">
    <entry name="main.rs" level="2" tokens="120" />
  </dir>
</project_map>
```

`hash` appears on project map entries only with `--file-hashes`. It is a SHA-256 hash of the original file content.

```xml
<entry path="src/main.rs" level="2" tokens="120" hash="sha256_hex" />
```

With `--no-token-counts`, token attributes are omitted:

```xml
<entry path="src/main.rs" level="2" />
```

## JSON

Default JSON shape:

```json
{
  "metadata": {
    "generated_at": "unix_seconds",
    "repo_root": "/path/to/repo",
    "max_tokens": 12000,
    "compression_level": 2,
    "file_count": 3
  },
  "project_map": [
    { "path": "src/main.rs", "level": 2, "tokens": 120 }
  ],
  "deleted_files": [
    { "path": "src/old.rs" }
  ],
  "directory_summaries": [
    { "path": "src", "files": 2, "tokens": 240 }
  ],
  "files": [
    {
      "path": "src/main.rs",
      "level": 2,
      "tokens": 120,
      "content": "compressed content"
    }
  ]
}
```

`directory_summaries` appears only with `--directory-summaries`.

`deleted_files` appears when incremental output detects files that were present in the baseline but are now gone.

`--no-content` omits `files`.

`--no-token-counts` omits `tokens` fields from `project_map`, `directory_summaries`, and `files`.

`--changed-since <git-ref>` uses `git diff` and emits added/changed files plus deleted file markers.

`--project-map-only` emits only the project map array:

```json
[
  { "path": "src/main.rs", "level": 2, "tokens": 120 }
]
```

`--project-map compact` groups entries by directory:

```json
[
  { "path": "src", "files": 2, "tokens": 240, "entries": [
    { "name": "main.rs", "level": 2, "tokens": 120 }
  ] }
]
```

`hash` appears on project map entries only with `--file-hashes`. It is a SHA-256 hash of the original file content.

```json
{ "path": "src/main.rs", "level": 2, "tokens": 120, "hash": "sha256_hex" }
```

With `--no-token-counts`, token fields are omitted:

```json
{ "path": "src/main.rs", "level": 2 }
```

## Text

Text output is lower overhead than XML/JSON and is meant for agent context, not strict parsing.

```text
bonsai_context
generated_at: unix_seconds
repo_root: /path/to/repo
max_tokens: 12000
compression_level: 2
file_count: 3

project_map
src/main.rs L2 tokens=120

deleted_files
src/old.rs

directory_summaries
src files=2 tokens=240

files
--- src/main.rs L2 tokens=120
compressed content
```

`--project-map-only` emits only the text project map. `--project-map compact` groups entries by directory:

```text
project_map compact
[src] files=2 tokens=240
main.rs L2 tokens=120
```

`--no-token-counts` omits `tokens=` fields.
