# TODO

## Install Story

- Support install with:

```sh
cargo install --path .
```

- Update plugins and VS Code extension to prefer the installed `contextshrink` binary.
- Add one clear smoke test for each integration:
  - CLI
  - Codex plugin
  - Claude Code plugin
  - VS Code extension

## CLI Quality

- Add `--include` and `--exclude` flags.
- Add `--respect-gitignore` toggle.
- Add `--print-files` to show selected files.
- Add `--fail-on-empty` for automation.
- Add clearer errors when no supported files are found.
