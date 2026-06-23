# TODO

## Output Quality

- Add deleted-file markers for incremental mode, so agents can see when important files disappeared.
- Add optional file hashes in the project map for stronger repeatability checks.
- Add `--changed-since <git-ref>` mode that uses git diff for branch review workflows.
- Add schema examples for incremental XML and JSON in `docs/output-schema.md`.

## Parser Coverage

- Add support for `.c`, `.h`, `.cpp`, `.hpp`, `.m`, and `.mm`.
- Add support for `.vue`, `.svelte`, `.astro`, and `.html`.
- Improve Markdown compression to preserve tables, links, and code fence language names.
- Improve JSON/YAML/TOML compression to keep top-level comments where supported.

## CLI Ergonomics

- Add `bonsai doctor` to show binary path, version, cache path, tokenizer, and parser availability.
- Add `--dry-run` to print selected files and estimated tokens without writing output.
- Add `--quiet` for scripts that only care about exit code and output file.
- Add shell completions for bash, zsh, and fish.

## Agent And Editor Integrations

- Add plugin smoke tests that verify generated agent instructions mention current flags.

## Testing And Release

- Add integration tests for incremental mode with deleted files and changed include/exclude filters.
- Add golden fixtures for JSON incremental output.
- Run `cargo fmt --check` in CI once current formatting drift is cleaned up.
- Add release smoke test that runs downloaded binaries with `--incremental`.
