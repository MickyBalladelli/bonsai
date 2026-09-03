# Bonsai CLI option reference

Generated from the Rust CLI schema. Regenerate this file with `scripts/generate-cli-reference.sh`.

## `bonsai`

```text
Shrink repository source context into token-efficient XML, JSON, or text

Usage: bonsai [OPTIONS] [PATH] [COMMAND]

Commands:
  setup        Check Bonsai and prepare the first-run output path
  init-agent   Write selectable AGENTS.md and CLAUDE.md starter instructions
  cache        Manage Bonsai cache
  doctor       Show install health
  completions  Generate shell completions
  docs         Print the generated Markdown CLI option reference
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

  -V, --version
          Print version

Basic:
      --config <PATH>
          Read settings from this file instead of the repository's .bonsai.toml

      --preset <PRESET>
          Simple flow: full (default), changed (local cache), map (project map), prompt (clipboard)
          
          [possible values: full, changed, map, prompt]

  [PATH]
          [default: .]

Budget:
      --max-tokens <MAX_TOKENS>
          [default: 4000]

      --tokenizer <TOKENIZER>
          Tokenizer family or model alias: o200k_base, cl100k_base, p50k_base, p50k_edit, r50k_base
          
          [default: cl100k_base]

      --max-file-bytes <MAX_FILE_BYTES>
          Skip files larger than this many bytes; 0 disables the cap
          
          [default: 1048576]

      --max-file-tokens <MAX_FILE_TOKENS>
          Cap each file to this many tokens before global budget optimization; 0 disables the cap

      --level <LEVEL>
          Compression: 1 keeps full source first, 2 keeps signatures and shapes, 3 keeps names and a tree map. Start with 2.
          
          [default: 2]

Output:
      --output <OUTPUT>
          [default: file]
          [possible values: clipboard, file]

      --output-file <OUTPUT_FILE>
          [default: bonsai.xml]

      --format <FORMAT>
          [default: xml]
          [possible values: json, text, xml]

      --project-map-only
          Write only the project map

      --map-only-under <TOKENS>
          When --max-tokens is below this value, output metadata, project map, and directory summaries only

      --project-map <PROJECT_MAP>
          [default: flat]
          [possible values: flat, compact]

      --file-hashes
          Include stable content hashes in project map entries

      --no-token-counts
          Omit token count fields from XML and JSON output

      --no-content
          Omit file bodies while keeping metadata and the project map

      --dry-run
          Print selected files and estimated tokens without writing output

      --sort <SORT>
          [default: path]
          [possible values: path, tokens, priority]

      --directory-summaries
          Add token totals for each directory

      --fail-over-budget
          Fail if the final output is still over budget

      --drop-low-priority
          Omit lowest-priority files if tree-map output still exceeds --max-tokens

Changes:
      --incremental
          Changed workflow: after one normal run, include only added or changed files from the local cache

      --incremental-base <PATH>
          Only include files added or changed compared with a base directory or cache file

      --changed-since <GIT_REF>
          Changed workflow: include tracked changes, untracked files, and deletions compared with this Git ref; do not combine with --incremental

      --incremental-summary
          Print added, changed, unchanged, skipped, and deleted counts

Selection:
      --include <GLOB>
          Only include matching paths

      --exclude <GLOB>
          Exclude matching paths

      --no-respect-gitignore
          Include files ignored by Git

      --exclude-generated
          Skip minified, vendored, generated, and lockfile-like files unless --include matches them

Diagnostics:
      --print-files
          Print selected files before generation

      --fail-on-empty
          Fail when no supported files are selected

      --quiet
          Suppress normal stdout output for scripts

      --stats
          Print a summary and output token count

      --detailed-stats
          Print per-file and token distribution details

      --summary
          Print output path and selected file count

Prompt:
      --prompt
          Wrap output in a paste-ready agent prompt

      --ask-template <TEXT>
          Use this task text inside the prompt wrapper

```

## `bonsai setup`

```text
Check Bonsai and prepare the first-run output path

Usage: setup [OPTIONS] [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
      --tokenizer <TOKENIZER>
          [default: cl100k_base]

      --output-file <OUTPUT_FILE>
          [default: bonsai.xml]

  -h, --help
          Print help

```

## `bonsai init-agent`

```text
Write selectable AGENTS.md and CLAUDE.md starter instructions

Usage: init-agent [OPTIONS] [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
  -f, --force
          

      --files <FILES>
          Create agents, claude, or both files
          
          [default: both]
          [possible values: agents, claude, both]

      --style <STYLE>
          Use short or detailed starter instructions
          
          [default: short]
          [possible values: short, detailed]

      --max-tokens <TOKENS>
          Add a token budget to the generated Bonsai command

      --output-file <PATH>
          Add a custom output file to the generated Bonsai command

  -h, --help
          Print help

```

## `bonsai cache`

```text
Manage Bonsai cache

Usage: cache <COMMAND>

Commands:
  clear  Clear the local parse cache for a repo
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

```

## `bonsai cache clear`

```text
Clear the local parse cache for a repo

Usage: clear [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
  -h, --help
          Print help

```

## `bonsai doctor`

```text
Show install health

Usage: doctor [OPTIONS] [PATH]

Arguments:
  [PATH]
          [default: .]

Options:
      --tokenizer <TOKENIZER>
          [default: cl100k_base]

      --json
          

  -h, --help
          Print help

```

## `bonsai completions`

```text
Generate shell completions

Usage: completions <SHELL>

Arguments:
  <SHELL>
          [possible values: bash, zsh, fish]

Options:
  -h, --help
          Print help

```

## `bonsai docs`

```text
Print the generated Markdown CLI option reference

Usage: docs

Options:
  -h, --help
          Print help

```
