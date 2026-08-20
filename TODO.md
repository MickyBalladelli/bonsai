# TODO

## VS Code CLI Parity

Make the VS Code extension expose every capability provided by the command line
version. Keep the chat, clipboard, preview, and status-bar features, but do not
remove CLI functionality.

### Configuration and generation

- Add a workspace path setting, including support for selecting a path instead
  of silently using only the first workspace folder.
- Match CLI defaults, especially `maxTokens` (`4000`) and output file
  (`bonsai.xml`), or make all defaults explicit and configurable.
- Add tokenizer selection with CLI tokenizer names and aliases.
- Add `maxFileBytes` and `maxFileTokens` settings.
- Add `text` to the output-format choices.
- Add clipboard output as an explicit destination, including raw context and
  prompt output.
- Add project-map-only output.
- Add compact and flat project-map modes.
- Add the map-only-under budget fallback.
- Add file hashes.
- Add switches for omitting token counts and file content.
- Add output sorting: `path`, `tokens`, and `priority`.
- Add directory summaries.
- Add fail-on-over-budget behavior.
- Add low-priority file dropping when the compressed result exceeds the budget.
- Add generated, vendored, minified, and lockfile exclusion.
- Add selected-file printing or an equivalent UI view.
- Add fail-on-empty-selection behavior with a visible error.
- Add quiet/script-friendly output mode where applicable.
- Add detailed statistics, including per-file tokens, extension totals, and
  min/max/mean/median data.
- Add the CLI prompt wrapper and configurable ask-template text.

### Incremental context

- Keep local `--incremental` support, but expose incremental summary counts in
  the UI.
- Add incremental comparison against another checkout with
  `--incremental-base`.
- Add incremental comparison against a cache file.
- Add git-ref change context with `--changed-since`.
- Show added, changed, unchanged, skipped, and deleted files.
- Show deleted-file entries in the project map and preview.
- Expose cache clearing for the current workspace and a selected repository.

### CLI subcommands and diagnostics

- Add an extension command for `init-agent`, including the `--force` option.
- Add an extension command for `cache clear`.
- Add an extension command for `doctor`, including text and JSON reports.
- Add an extension command or downloadable files for shell completions:
  Bash, Zsh, and Fish.
- Add a visible Bonsai version and binary-health report.

### Binary and workspace behavior

- Make binary discovery work consistently on all supported platforms and
  report a clear install error when no binary is available.
- Pass the same environment and working-directory behavior as the CLI.
- Support multi-root workspaces and let the user choose the repository.
- Preserve CLI exit errors and warnings in VS Code notifications or an output
  channel instead of losing them.

### Parity checks

- Add automated extension coverage for every exposed CLI option.
- Add a parity test comparing extension-generated arguments with the CLI help
  surface, so new CLI flags cannot be missed.
- Verify XML, JSON, and text outputs, clipboard output, prompts, incremental
  modes, deleted files, project-map modes, budget failures, and diagnostics.
