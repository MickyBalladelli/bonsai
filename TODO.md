# TODO

## Token Cost Improvements

- Collapse long import/include/use blocks by language: keep first few, then `... N more imports`.
- Make Markdown/config/web truncation token-aware instead of character-aware.
- Add Markdown long-list sampling, like table sampling: keep first items plus tail items.
- Add config allowlist tuning for common files: `package.json`, GitHub workflows, `Cargo.toml`, plugin manifests, VS Code manifests.
- Add per-file `--max-file-tokens` to cap huge single files before global budget optimization.
- Add `--map-only-under <tokens>` fallback for tiny budgets: output project map and directory summaries only.
- Add golden token-cost fixtures to catch regressions for large tables, large configs, import-heavy code, and many-file repos.
