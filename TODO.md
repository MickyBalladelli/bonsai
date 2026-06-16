# TODO

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

## Later

- Add `--tokenizer` for common model families if practical.
- Cache parsed file variants by mtime and size.
- Add incremental mode for repeated local runs.
