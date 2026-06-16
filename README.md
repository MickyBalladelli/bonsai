<img src="images/context-shrink.png" alt="ContextShrink logo" width="160">

# ContextShrink

ContextShrink reduces the number of tokens needed to run LLM queries over a codebase. It walks a repo, parses code with tree-sitter, shrinks source into skeletons or tree maps, counts tokens, then writes XML for LLM context.

## Build

Install Rust first:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then build:

```sh
cargo build --release
```

Binary appears here:

```sh
target/release/contextshrink
```

## Run

Scan current directory and write `contextshrink.xml`:

```sh
target/release/contextshrink . --max-tokens 4000 --level 2 --output file
```

Copy XML to clipboard:

```sh
target/release/contextshrink . --max-tokens 4000 --level 2 --output clipboard
```

Pick another output file:

```sh
target/release/contextshrink . --max-tokens 8000 --level 1 --output file --output-file repo-context.xml
```

Write JSON instead of XML:

```sh
target/release/contextshrink . --max-tokens 12000 --level 2 --format json --output file --output-file /tmp/contextshrink.json
```

Print a run summary:

```sh
target/release/contextshrink . --max-tokens 12000 --level 2 --output file --output-file /tmp/contextshrink.xml --summary
```

Print token savings stats:

```sh
target/release/contextshrink . --max-tokens 12000 --level 2 --output file --output-file /tmp/contextshrink.xml --stats
```

## Levels

`--level 1` keeps full code first, then shrinks files if token budget is too small.

`--level 2` keeps imports, signatures, types, classes, and function shapes. Function bodies become `...`.

`--level 3` keeps a compact tree map only.

## Measuring Token Savings

Use `--stats` to measure how much ContextShrink saves:

```sh
target/release/contextshrink . --max-tokens 12000 --level 2 --output file --output-file /tmp/contextshrink.xml --stats
```

Example output:

```text
stats:
  raw_tokens: 50000
  shrunk_tokens: 9000
  tokens_saved: 41000
  saving_percent: 82.00
  files_scanned: 42
```

ContextShrink compares full XML against shrunk XML with the same tokenizer it uses for budgeting.

```text
tokens_saved = raw_tokens - shrunk_tokens
saving_percent = tokens_saved / raw_tokens * 100
```

## Output Format

XML is the default output. Use `--format json` for JSON.

Both formats include:

```text
metadata: generated time, repo root, token budget, compression level, file count
project map: file paths, selected levels, per-file token counts
files: compressed file contents with per-file token counts
```

## Supported Files

ContextShrink scans:

```text
.js .jsx .ts .tsx .py .rs .go .java .cs .swift .kt .md .json .yaml .yml .toml
```

It parses JavaScript, TypeScript, Python, and Rust with tree-sitter. Other code languages use generic declaration extraction. Docs and config files use compact line-based context instead of source-code body stripping.

It respects `.gitignore` and `.cursorignore`.

## Development Check

```sh
cargo check
```

## Run Tests

Run unit tests:

```sh
cargo test
```

Run one test by name:

```sh
cargo test parser::tests::rust_skeleton_strips_function_body
```

## Full Local Test

From repo root:

```sh
cd "$HOME/dev/context-shrink"
```

Build:

```sh
cargo build --release
```

Run CLI:

```sh
target/release/contextshrink . --max-tokens 2000 --level 2 --output file --output-file /tmp/contextshrink.xml
```

Inspect output:

```sh
head -40 /tmp/contextshrink.xml
```

Run plugin helper:

```sh
plugins/contextshrink/skills/contextshrink/scripts/run_contextshrink.sh . 2000 2 /tmp/contextshrink-plugin.xml
```

Inspect plugin output:

```sh
head -40 /tmp/contextshrink-plugin.xml
```

Expected first line:

```xml
<repository_context>
```

## Codex Plugin

This repo includes a Codex plugin copy:

```text
plugins/contextshrink
```

It adds a `$contextshrink` skill. Codex can run the CLI, write `/tmp/contextshrink.xml`, read it, then answer with compressed repo context.

Use it in Codex:

```text
Use $contextshrink to compress this repo before answering.
```

Helper command:

```sh
plugins/contextshrink/skills/contextshrink/scripts/run_contextshrink.sh . 12000 2 /tmp/contextshrink.xml
```

## Install Plugin In Codex

Users install the plugin through the repo marketplace file:

```text
.agents/plugins/marketplace.json
```

The marketplace points to:

```text
plugins/contextshrink
```

Build the CLI first:

```sh
cargo build --release
```

Add the marketplace to Codex:

```sh
codex plugin marketplace add "$HOME/dev/context-shrink/.agents/plugins"
```

If your Codex CLI expects the JSON file directly, use:

```sh
codex plugin marketplace add "$HOME/dev/context-shrink/.agents/plugins/marketplace.json"
```

Then open Codex app and install or enable `contextshrink`.

Use it:

```text
Use $contextshrink to compress this repo before answering.
```

Expected behavior:

```text
Ask: summarize this whole project
See: ContextShrink command execute
Inspect: /tmp/contextshrink.xml
Answer: summary uses compressed repository context
```

The plugin helper tries to build the release binary if it is missing, but building first makes the test clearer.

## Claude Code Plugin

This repo includes a Claude Code plugin:

```text
claude/contextshrink
```

It adds a namespaced Claude Code skill:

```text
/contextshrink:contextshrink
```

The skill runs ContextShrink, writes `/tmp/contextshrink.xml`, then Claude Code reads the XML before answering.

This repo also includes:

```text
CLAUDE.md
```

That tells Claude Code to run ContextShrink before repo-wide analysis, including full project summaries.

Build the CLI first:

```sh
cargo build --release
```

Test the plugin from repo root:

```sh
claude --plugin-dir ./claude/contextshrink
```

If Claude Code is already open, restart it with `--plugin-dir` or install the plugin before expecting automatic skill use.

Inside Claude Code, run:

```text
/contextshrink:contextshrink
```

Helper command:

```sh
claude/contextshrink/bin/contextshrink-claude . 12000 2 /tmp/contextshrink.xml
```

Expected behavior:

```text
Ask: summarize this whole project
See: ContextShrink command execute
Inspect: /tmp/contextshrink.xml
Answer: summary uses compressed repository context
```

### Publish Claude Code Marketplace

This repo includes a Claude Code marketplace file:

```text
.claude-plugin/marketplace.json
```

For local testing, add the marketplace from the repo root:

```sh
claude plugin marketplace add .
```

Then install the plugin inside Claude Code:

```text
/plugin install contextshrink@context-shrink
```

Validate before publishing:

```sh
claude plugin validate .
```

For publishing, push this repo to a git host. Users can add it with:

```sh
claude plugin marketplace add owner/repo
```

## VS Code Extension

This repo includes a VS Code extension for generating ContextShrink XML for Copilot Chat, ChatGPT, or Codex in VS Code:

```text
copilot/contextshrink-vscode
```

It adds Command Palette actions:

```text
ContextShrink: Generate Context
ContextShrink: Generate and Ask
ContextShrink: Copy Context Prompt
ContextShrink: Open Last Context
```

`Generate Context` writes XML, opens it, and copies a short prompt.

`Generate and Ask` writes XML, opens it, copies the full prompt, and opens VS Code chat when available.

`Copy Context Prompt` writes XML and copies the full XML prompt to clipboard, ready to paste into Copilot Chat, ChatGPT, or Codex in VS Code.

`Open Last Context` opens the last generated output file.

This repo also includes:

```text
.github/copilot-instructions.md
```

That tells Copilot how to treat ContextShrink XML when it sees it.

### Build VS Code Extension

Build ContextShrink first:

```sh
cargo build --release
```

Build the VS Code extension:

```sh
cd copilot/contextshrink-vscode
npm install
npm run compile
```

Package a `.vsix`:

```sh
npm run package
```

The package appears as:

```text
contextshrink-vscode-0.1.0.vsix
```

### Install VS Code Extension

From repo root, install into VS Code:

```sh
"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" --install-extension copilot/contextshrink-vscode/contextshrink-vscode-0.1.0.vsix
```

Install into Cursor:

```sh
code --install-extension copilot/contextshrink-vscode/contextshrink-vscode-0.1.0.vsix
```

On some machines, `code` points to Cursor. Use the full VS Code path above when you want Visual Studio Code.

Or use the app UI:

```text
Extensions → ... → Install from VSIX...
```

Pick:

```text
copilot/contextshrink-vscode/contextshrink-vscode-0.1.0.vsix
```

Restart VS Code after installing.

### Use With VS Code Chat

Open the repo in VS Code.

Run Command Palette:

```text
ContextShrink: Generate and Ask
```

If chat does not open automatically, paste the copied prompt into Copilot Chat, ChatGPT, or Codex in VS Code.

Smoke test:

```text
Using this context, explain what src/main.rs does.
```

If Copilot answers from the XML, the extension works.

For smaller clipboard payload, run:

```text
ContextShrink: Generate Context
```

Then ask VS Code chat:

```text
Use the opened ContextShrink XML as compressed repo context and summarize the architecture.
```

Settings:

```text
contextshrink.maxTokens
contextshrink.level
contextshrink.outputFile
contextshrink.binaryPath
```

## How The Plugin Was Created

Scaffold plugin:

```sh
python3 "$HOME/.codex/skills/.system/plugin-creator/scripts/create_basic_plugin.py" contextshrink --with-skills --with-marketplace
```

Scaffold skill:

```sh
python3 "$HOME/.codex/skills/.system/skill-creator/scripts/init_skill.py" contextshrink --path "$HOME/plugins/contextshrink/skills" --resources scripts --interface display_name=ContextShrink --interface short_description='Compress repo context for Codex prompts' --interface default_prompt='Use $contextshrink to compress this repo into XML context before answering.'
```

Then files were copied into this repo under `plugins/contextshrink` so git can save them.

Personal install lives here:

```text
$HOME/plugins/contextshrink
```

Personal marketplace entry lives here:

```text
$HOME/.agents/plugins/marketplace.json
```
