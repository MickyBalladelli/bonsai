# Bonsai Output Schema

Bonsai writes XML by default. JSON is available with `--format json`.

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

`--project-map-only` emits only:

```xml
<project_map>
  <entry path="src/main.rs" level="2" tokens="120" />
</project_map>
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

`--project-map-only` emits only the project map array:

```json
[
  { "path": "src/main.rs", "level": 2, "tokens": 120 }
]
```
