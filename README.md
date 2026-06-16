<p align="center">
    <img src="images/bonsai.png" alt="Bonsai logo" width="160" />
</p>

<p align="center">
    <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-blue.svg" /></a>
    <img alt="Language: Rust" src="https://img.shields.io/badge/Language-Rust-orange.svg" />
    <img alt="Version" src="https://img.shields.io/badge/Version-0.2.1-lightgrey.svg" />
</p>

# Bonsai

  On this repository Bonsai reduced ~76,911 raw tokens to 13,450 shrunk tokens (~82.5% savings) using AST skeletonization — keep your LLM contexts small, accurate, and cheap.*

Naming note: the GitHub repository, CLI binary, Codex plugin, and Claude plugin are `bonsai`; the VS Code package is `bonsai-vscode`.

## Examples

Command panel:

![Command panel](images/panel_cmd.png)

Stats panel:

![Stats panel](images/panel_stats.png)

## Quick Start

Run inside a repo:

```sh
bonsai .
```

This writes:

```text
bonsai.xml
```

Copy it into your LLM and ask:

```text
Use this Bonsai XML as repo context. Summarize the architecture and tell me where to start reading.
```

Need the binary? Download a release. No Rust needed.

macOS Apple Silicon:

```sh
curl -L -o bonsai https://github.com/MickyBalladelli/bonsai/releases/latest/download/bonsai-macos-arm64
chmod +x bonsai
sudo mv bonsai /usr/local/bin/bonsai
```

Linux x64:

```sh
curl -L -o bonsai https://github.com/MickyBalladelli/bonsai/releases/latest/download/bonsai-linux-x64
chmod +x bonsai
sudo mv bonsai /usr/local/bin/bonsai
```

Or install from source:

```sh
cargo install --path .
```

Install straight from GitHub:

```sh
cargo install --git https://github.com/MickyBalladelli/bonsai.git
```

## What It Is Good For

Use Bonsai when you want broad repo context that is visible and repeatable:

```text
Summarize this project.
Explain the architecture.
Find likely entry points.
Prepare context before asking another LLM.
Compare token savings before sending repo context.
```

It is less useful when the agent already has the exact file or function you want edited.

## Install

Release binaries:

```text
bonsai-macos-arm64
bonsai-linux-x64
```

Download from:

```text
https://github.com/MickyBalladelli/bonsai/releases/latest
```

From this repo:

```sh
cargo install --path .
```

From git:

```sh
cargo install --git https://github.com/MickyBalladelli/bonsai.git
```

Or build a local binary:

```sh
cargo build --release
target/release/bonsai .
```

Tagged releases publish:

```text
bonsai-linux-x64
bonsai-macos-arm64
bonsai-linux-x64.sha256
bonsai-macos-arm64.sha256
bonsai-vscode-*.vsix
```

The Codex plugin, Claude Code plugin, and VS Code extension find the binary in this order:

```text
BONSAI_BIN
bonsai on PATH
repo-local target/release/bonsai
```

## Advanced Commands

Paste-ready clipboard prompt:

```sh
bonsai . --prompt --output clipboard
```

Custom paste-ready prompt:

```sh
bonsai . --ask-template "Use this repo context to find likely bugs." --output clipboard
```

Architecture map:

```sh
bonsai . --level 3
```

Detailed repo context:

```sh
bonsai . --max-tokens 12000 --level 2
```

Only scan `src`:

```sh
bonsai src
```

Write somewhere else:

```sh
bonsai . --output-file /tmp/bonsai.xml
```

Write JSON:

```sh
bonsai . --format json --output-file /tmp/bonsai.json
```

Write a paste-ready prompt file:

```sh
bonsai . --prompt --output-file /tmp/bonsai-prompt.txt
```

Show selected files:

```sh
bonsai . --print-files
```

Only write the project map:

```sh
bonsai . --project-map-only
```

Write metadata and project map without file bodies:

```sh
bonsai . --no-content
```

Sort output:

```sh
bonsai . --sort priority
bonsai . --sort tokens
bonsai . --sort path
```

Add per-directory token summaries:

```sh
bonsai . --directory-summaries
```

Fail when output is still over budget after maximum compression:

```sh
bonsai . --max-tokens 12000 --fail-over-budget
```

Write starter agent instructions:

```sh
bonsai init-agent
bonsai init-agent . --force
```

Measure token savings:

```sh
bonsai . --max-tokens 12000 --stats
```

Filter files:

```sh
bonsai . --include 'src/**' --exclude '**/generated.rs'
```

Skip files larger than the default 1 MiB limit:

```sh
bonsai . --max-file-bytes 2097152
```

Use `--max-file-bytes 0` to disable the size cap.

## Use With Agents

### Plain Paste

Generate context:

```sh
bonsai . --max-tokens 12000 --level 2 --prompt --output-file /tmp/bonsai-prompt.txt
```

Paste `/tmp/bonsai-prompt.txt` into an LLM. It starts with:

```text
Use this repo context to explain the architecture, identify the main entry points, and tell me where to start reading.
```

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

The helper command is:

```sh
plugins/bonsai/skills/bonsai/scripts/run_bonsai.sh . 12000 2 /tmp/bonsai.xml
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

The helper command is:

```sh
claude/bonsai/bin/bonsai-claude . 12000 2 /tmp/bonsai.xml
```

For local marketplace testing:

```sh
claude plugin marketplace add .
```

Then inside Claude Code:

```text
/plugin install bonsai@bonsai
```

### VS Code

The VS Code extension lives here:

```text
copilot/bonsai-vscode
```

Install the packaged VSIX:

```sh
code --install-extension copilot/bonsai-vscode/bonsai-vscode-0.2.1.vsix
```

Run Command Palette:

```text
Bonsai: Generate and Ask
```

If chat does not open automatically, paste the copied prompt into Copilot Chat, ChatGPT, or Codex in VS Code.

Other commands:

```text
Bonsai: Generate Context
Bonsai: Copy Context Prompt
Bonsai: Copy Project Map
Bonsai: Preview Project Map
Bonsai: Open Last Context
```

<img src="images/vscode-flow.svg" alt="Bonsai VS Code flow" width="720">

## Levels

`--level 1` keeps full code first, then shrinks files if the token budget is too small.

`--level 2` keeps imports, signatures, types, classes, and function shapes. Function bodies become `...`.

`--level 3` keeps a compact tree map only.

## Before And After

Full source:

```rust
fn greet(name: &str) -> String {
    let message = format!("hello {name}");
    println!("{message}");
    message
}
```

Skeleton:

```rust
fn greet(name: &str) -> String { ... }
```

Tree map:

```text
fn greet(name: &str) -> String
```

Markdown and config files are treated differently from source code. Bonsai keeps compact headings, important lines, and top-level config shape.

## Output Format

XML is the default. Use `--format json` for JSON.

Both formats include:

```text
metadata: generated time, repo root, token budget, compression level, file count
project map: file paths, selected levels, per-file token counts
files: compressed file contents with per-file token counts
```

Schema notes live in:

```text
docs/output-schema.md
```

## Supported Files

Bonsai scans:

```text
.js .jsx .ts .tsx .py .rs .go .java .cs .swift .kt .md .json .yaml .yml .toml
```

It parses JavaScript, TypeScript, Python, Rust, Go, Java, C#, Swift, and Kotlin with tree-sitter. Docs and config files use compact line-based context.

It respects `.gitignore` and `.cursorignore`.

## Troubleshooting

Binary not found:

```text
Install with `cargo install --path .`, set `BONSAI_BIN`, or run `cargo build --release`.
```

Clipboard failure:

```text
Use `--output file --output-file /tmp/bonsai.xml`.
Clipboard access can fail in headless shells, remote sessions, or sandboxes.
```

No files selected:

```text
Run with `--print-files`.
Check `--include`, `--exclude`, `.gitignore`, and `.cursorignore`.
Use `--no-respect-gitignore` when ignored files should be included.
```

Output over budget:

```text
Use a smaller target path, add `--exclude`, increase `--max-tokens`, or use `--level 3`.
```

## Development

Check:

```sh
cargo check
```

Test:

```sh
cargo test
```

Build release binary:

```sh
cargo build --release
```

Build VS Code extension:

```sh
cd copilot/bonsai-vscode
npm install
npm run compile
npm run package
```

## Release

CI and releases are configured in:

```text
.github/workflows/ci.yml
.github/workflows/release.yml
```

Create a release:

```sh
git tag v0.2.1
git push origin v0.2.1
```

Version bump checklist:

```text
Cargo.toml
copilot/bonsai-vscode/package.json
copilot/bonsai-vscode/package-lock.json
plugins/bonsai/.codex-plugin/plugin.json
claude/bonsai/.claude-plugin/plugin.json
.claude-plugin/marketplace.json, if pinning marketplace version
README install/package examples
```
