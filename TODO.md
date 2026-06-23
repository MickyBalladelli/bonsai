# TODO

## Output Quality


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

## Testing And Release

- Add release smoke test that runs downloaded binaries with `--incremental`.
