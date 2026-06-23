<p align="center">
  <img src="images/bonsai.png" alt="Bonsai logo" width="160" />
</p>

<p align="center">
  <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-blue.svg" /></a>
  <img alt="Language: Rust" src="https://img.shields.io/badge/Language-Rust-orange.svg" />
  <img alt="Version" src="https://img.shields.io/badge/Version-0.4.2-lightgrey.svg" />
</p>

# Bonsai

Bonsai turns a local repository into a small, repeatable context file for LLMs.

It scans source files, compresses code with syntax-aware summaries, respects your token budget, and writes XML or JSON you can paste into ChatGPT, Codex, Claude, Copilot, or another agent.

Use it when you want an LLM to understand a whole project before asking for architecture, onboarding, review, or branch-change help.

## Quick Start

Install Bonsai, then run it inside a repository:

```sh
bonsai .
```

This writes:

```text
bonsai.xml
```

Paste that file into an LLM and ask:

```text
Use this Bonsai repo context. Explain the architecture and tell me where to start reading.
```

For a larger context budget:

```sh
bonsai . --max-tokens 12000 --level 2 --output-file /tmp/bonsai.xml
```

For a paste-ready prompt:

```sh
bonsai . --prompt --output-file /tmp/bonsai-prompt.txt
```

## Quick Setup For Agents

Run this once in any repo where you want agents to use Bonsai first:

```sh
bonsai init-agent
```

It writes:

```text
AGENTS.md
CLAUDE.md
```

Those files tell Codex, Claude Code, and similar agents to run Bonsai before answering broad project questions.

Overwrite existing files:

```sh
bonsai init-agent --force
```

## Install

Download a release binary:

```text
https://github.com/mickyhq/bonsai/releases/latest
```

Release assets:

```text
bonsai-linux-x64
bonsai-macos-arm64
bonsai-linux-x64.sha256
bonsai-macos-arm64.sha256
bonsai-vscode-*.vsix
```

macOS Apple Silicon:

```sh
curl -L -o bonsai https://github.com/mickyhq/bonsai/releases/latest/download/bonsai-macos-arm64
chmod +x bonsai
sudo mv bonsai /usr/local/bin/bonsai
```

Linux x64:

```sh
curl -L -o bonsai https://github.com/mickyhq/bonsai/releases/latest/download/bonsai-linux-x64
chmod +x bonsai
sudo mv bonsai /usr/local/bin/bonsai
```

Install from this checkout:

```sh
cargo install --path .
```

Install from GitHub:

```sh
cargo install --git https://github.com/mickyhq/bonsai.git
```

Check your install:

```sh
bonsai doctor
```

## What Bonsai Keeps

Bonsai has three compression levels:

```text
--level 1  Full code first, then shrink if needed
--level 2  Imports, signatures, types, classes, and function shapes
--level 3  Compact tree map only
```

Example source:

```rust
fn greet(name: &str) -> String {
    let message = format!("hello {name}");
    println!("{message}");
    message
}
```

Level 2 skeleton:

```rust
fn greet(name: &str) -> String { ... }
```

Level 3 tree map:

```text
fn greet(name: &str) -> String
```

Markdown keeps headings, useful summary text, tables, links, and code fence language names. Config files keep important top-level shape and supported top-level comments.

## Common Commands

Write XML to `bonsai.xml`:

```sh
bonsai .
```

Write JSON:

```sh
bonsai . --format json --output-file /tmp/bonsai.json
bonsai . --format text --output-file /tmp/bonsai.txt
```

Copy a prompt to the clipboard:

```sh
bonsai . --prompt --output clipboard
```

Use a model-family tokenizer:

```sh
bonsai . --tokenizer gpt-4o
bonsai . --tokenizer o200k_base
```

Make a compact architecture map:

```sh
bonsai . --level 3
```

Write only the project map:

```sh
bonsai . --project-map-only
bonsai . --project-map compact --project-map-only
```

Include stable file hashes in the project map:

```sh
bonsai . --file-hashes
```

Omit token count fields from XML/JSON:

```sh
bonsai . --no-token-counts
```

Write metadata and project map without file bodies:

```sh
bonsai . --no-content
```

Show selected files:

```sh
bonsai . --print-files
```

Preview selected files and estimated tokens without writing output:

```sh
bonsai . --dry-run
```

Suppress normal stdout for scripts:

```sh
bonsai . --quiet --output-file /tmp/bonsai.xml
```

Generate shell completions:

```sh
bonsai completions bash > ~/.local/share/bash-completion/completions/bonsai
bonsai completions zsh > ~/.zfunc/_bonsai
bonsai completions fish > ~/.config/fish/completions/bonsai.fish
```

Filter files:

```sh
bonsai . --include 'src/**' --exclude '**/generated.rs'
bonsai . --exclude-generated
```

`--exclude-generated` skips minified, vendored, generated, and lockfile-like files. A matching `--include` pattern keeps explicit paths.

Sort output:

```sh
bonsai . --sort priority
bonsai . --sort tokens
bonsai . --sort path
```

Add directory token summaries:

```sh
bonsai . --directory-summaries
```

Fail if output cannot fit after maximum compression:

```sh
bonsai . --max-tokens 12000 --fail-over-budget
```

Drop lowest-priority files if maximum compression still does not fit:

```sh
bonsai . --max-tokens 12000 --drop-low-priority
```

## Change-Focused Context

Only include files changed since the last cached local run:

```sh
bonsai . --incremental
```

Show the incremental counts:

```sh
bonsai . --incremental --incremental-summary
```

Compare with another checkout or cache file:

```sh
bonsai . --incremental-base /path/to/base/repo
bonsai . --incremental-base /path/to/base.cache
```

Include tracked changes and untracked files against a git ref:

```sh
bonsai . --changed-since main
```

Clear the local parse cache for a repo:

```sh
bonsai cache clear
bonsai cache clear /path/to/repo
```

Bonsai stores file-selection options with the cache. If `--include`, `--exclude`, `--exclude-generated`, `--max-file-bytes`, or gitignore handling changes, the next incremental run includes selected files once instead of comparing against stale selection.

## Output

XML is default. JSON is available with `--format json`. Lower-overhead text is available with `--format text`.

Output includes:

```text
metadata     generated time, repo root, token budget, level, file count
project_map  file path, selected level, token count, optional hash
files        compressed file content and per-file token count
```

Use `--no-token-counts` to omit token count fields from XML/JSON output.

Schema details:

```text
docs/output-schema.md
```

## Supported Files

Bonsai scans:

```text
.js .jsx .ts .tsx .py .rs .go .java .cs .swift .kt
.c .h .cpp .hpp .m .mm
.vue .svelte .astro .html
.md .json .yaml .yml .toml
```

Tree-sitter parsers:

```text
JavaScript, TypeScript, Python, Rust, Go, Java, C#, Swift, Kotlin, C, C++
```

Compact structure extraction:

```text
Objective-C, web templates, Markdown, JSON, YAML, TOML
```

Bonsai respects `.gitignore` and `.cursorignore` by default.

## Use With Codex, Claude, And VS Code

### Codex

This repo includes a Codex plugin:

```text
plugins/bonsai
```

Add the local marketplace:

```sh
codex plugin marketplace add "$HOME/dev/bonsai/.agents/plugins"
```

Then install or enable `bonsai` in Codex.

Ask:

```text
Use $bonsai to compress this repo before answering.
```

### Claude Code

This repo includes a Claude Code plugin:

```text
claude/bonsai
```

Run Claude Code with the plugin:

```sh
claude --plugin-dir ./claude/bonsai
```

Use the skill:

```text
/bonsai:bonsai
```

### VS Code

The VS Code extension lives here:

```text
copilot/bonsai-vscode
```

Install the packaged VSIX:

```sh
code --install-extension copilot/bonsai-vscode/bonsai-vscode-0.4.2.vsix
```

Command Palette commands:

```text
Bonsai: Generate and Ask
Bonsai: Generate Context
Bonsai: Copy Context Prompt
Bonsai: Copy Changed Context
Bonsai: Copy Project Map
Bonsai: Preview Project Map
Bonsai: Open Last Context
```

<img src="images/vscode-flow.svg" alt="Bonsai VS Code flow" width="720">

## Examples

Command palette:

![Command panel](images/panel_cmd.png)

Output stats:

![Stats panel](images/panel_stats.png)

## Troubleshooting

Binary not found:

```text
Install Bonsai, put it on PATH, set BONSAI_BIN, or run cargo build --release.
```

Clipboard failure:

```text
Use --output file --output-file /tmp/bonsai.xml.
Clipboard access can fail in headless shells, remote sessions, or sandboxes.
```

No files selected:

```text
Run with --print-files.
Check --include, --exclude, .gitignore, and .cursorignore.
Use --no-respect-gitignore if ignored files should be included.
```

Output over budget:

```text
Use a smaller path, add --exclude, increase --max-tokens, or use --level 3.
```

Check parser and tokenizer health:

```sh
bonsai doctor
bonsai doctor --json
```

It also reports the cache path, cache size, cache entry count, stored selection metadata, and stale entries.

## Development

Check:

```sh
cargo check
```

Test:

```sh
cargo test
```

Build:

```sh
cargo build --release
```

Build the VS Code extension:

```sh
cd copilot/bonsai-vscode
npm install
npm run compile
npm run package
```

## Names

The GitHub repository, CLI binary, Codex plugin, and Claude plugin are named `bonsai`.

The VS Code package is named `bonsai-vscode`.
