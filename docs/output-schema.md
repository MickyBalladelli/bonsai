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
  <directory_summaries>
    <directory path="src" files="2" tokens="240" />
  </directory_summaries>
  <files>
    <file path="src/main.rs" level="2" tokens="120">compressed content</file>
  </files>
</repository_context>
```

`directory_summaries` appears only with `--directory-summaries`.

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

`--no-content` omits `files`.

`--project-map-only` emits only the project map array:

```json
[
  { "path": "src/main.rs", "level": 2, "tokens": 120 }
]
```
