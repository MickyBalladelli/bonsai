# TODO

## Correctness And Compression

- Add true parser support for generic languages now handled by line heuristics:
  - Go
  - Java
  - C#
  - Swift
  - Kotlin
- Improve Markdown compression:
  - keep headings with nearby summary text
  - keep important code blocks
  - drop badges and noisy generated sections
- Improve JSON/YAML/TOML compression:
  - keep top-level keys
  - collapse long arrays
  - preserve package/script/dependency sections
- Add file-level priority scoring so entry points and manifests survive tight budgets before leaf files.
- Add a budget mode that reserves fixed tokens for project map and metadata before file contents.

## Token Accounting

- Count final output tokens after metadata, project map, and wrappers.
- Add per-section token counts:
  - metadata
  - project map
  - files
- Add `--tokenizer` option for common model families if practical.
- Add a warning when output still exceeds `--max-tokens` after all files downgrade to tree map.

## Output Quality

- Add `--project-map-only`.
- Add `--no-content` for metadata and project map without file bodies.
- Add `--sort` modes:
  - path
  - tokens
  - priority
- Add optional per-directory summaries.
- Add schema docs for XML and JSON output.

## Testing

- Add CLI integration tests that execute the binary with temp repos.
- Add golden-file snapshots for XML and JSON output.
- Add tests for:
  - `--include`
  - `--exclude`
  - `--no-respect-gitignore`
  - `--print-files`
  - `--fail-on-empty`
- Add tests for empty repos and repos with only unsupported files.

## Performance

- Avoid formatting full raw context when only stats are not requested.
- Cache parsed file variants by mtime and size.
- Skip very large files by default with an override flag.
- Add `--max-file-bytes`.



Make install names stable
Repo is bonsai-context, binary is bonsai, plugin folders vary. Pick one main name everywhere.

Add examples by goal
Not flag list first. Use:
I want architecture: bonsai . --level 3
I want detailed review: bonsai . --max-tokens 12000
I want src only: bonsai src

Improve VS Code README
Show “install VSIX, open command palette, run Generate and Ask”. Hide packaging stuff lower.

Add bonsai init-agent
It could write AGENTS.md / CLAUDE.md instructions automatically. Very good for “use this with Codex/Claude” path.