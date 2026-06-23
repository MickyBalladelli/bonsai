# TODO

## CLI Ergonomics

- Convert `doctor`, `cache`, `init-agent`, and `completions` to real Clap subcommands.

## Change Workflows

- Add untracked-file support for `--changed-since` branch review workflows.

## Output Quality

- Improve JSON/YAML/TOML compression to preserve nested important sections better.
- Improve Markdown table compression to keep headers plus sampled rows for long tables.

## Parser Coverage

- Add stronger Objective-C / Objective-C++ structure extraction for `.m` and `.mm`.

## Testing And Release

- Move release binary incremental smoke into a shared test script used by release workflow.
- Add golden tests for `--dry-run`, `--quiet`, `--changed-since`, `--file-hashes`, and completions.
