# TODO

## CLI And Output

- Add `--project-map-only`.
- Add `--no-content` for metadata plus project map without file bodies.
- Add `--sort path|tokens|priority`.
- Add optional per-directory summaries.
- Add schema docs for XML and JSON output.
- Add `bonsai init-agent` to write starter `AGENTS.md` / `CLAUDE.md` instructions for using Bonsai before broad repo questions.
- Consider a `--fail-over-budget` flag for CI and agent workflows.

## Tests

- Add CLI integration tests that run the compiled binary against temp repos.
- Add golden snapshots for XML and JSON output.
- Add CLI coverage for:
  - `--include`
  - `--exclude`
  - `--no-respect-gitignore`
  - `--print-files`
  - `--fail-on-empty`
  - `--prompt`
  - `--ask-template`
  - `--format json`
- Add tests for empty repos, repos with only unsupported files, invalid globs, and unreadable files.
- Add tests proving XML and JSON token counts match the final emitted document.

## VS Code And Plugins

- Add plugin smoke tests for binary lookup order:
  - `BONSAI_BIN`
  - `bonsai` on `PATH`
  - repo-local `target/release/bonsai`
- Check release workflow publishes VSIX and both binary checksums with names that match README.

## Later

- Add `--tokenizer` for common model families if practical.
- Cache parsed file variants by mtime and size.
- Add incremental mode for repeated local runs.
