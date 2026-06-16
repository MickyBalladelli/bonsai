# ContextShrink

ContextShrink is local CLI tool. It walks a repo, parses code with tree-sitter, shrinks source into skeletons or tree maps, counts tokens, then writes XML for LLM context.

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

## Levels

`--level 1` keeps full code first, then shrinks files if token budget is too small.

`--level 2` keeps imports, signatures, types, classes, and function shapes. Function bodies become `...`.

`--level 3` keeps a compact tree map only.

## Supported Files

ContextShrink scans:

```text
.js .jsx .ts .tsx .py .rs
```

It respects `.gitignore` and `.cursorignore`.

## Development Check

```sh
cargo check
```

## Full Local Test

From repo root:

```sh
cd "$HOME/dev/bonsai"
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
codex plugin marketplace add "$HOME/dev/bonsai/.agents/plugins"
```

If your Codex CLI expects the JSON file directly, use:

```sh
codex plugin marketplace add "$HOME/dev/bonsai/.agents/plugins/marketplace.json"
```

Then open Codex app and install or enable `contextshrink`.

Use it:

```text
Use $contextshrink to compress this repo before answering.
```

The plugin helper tries to build the release binary if it is missing, but building first makes the test clearer.

## Copilot Plugin

GitHub Copilot does not use Codex plugins. For Copilot, this repo includes a VS Code extension:

```text
copilot/contextshrink-vscode
```

It adds Command Palette actions:

```text
ContextShrink: Generate Copilot Context
ContextShrink: Copy Copilot Context
ContextShrink: Open Last Context
```

`Generate` writes XML and opens it. It copies a short prompt telling Copilot to use the opened XML.

`Copy` writes XML and copies the full XML prompt to clipboard, ready to paste into Copilot Chat.

`Open Last Context` opens the last generated output file.

This repo also includes:

```text
.github/copilot-instructions.md
```

That tells Copilot how to treat ContextShrink XML when it sees it.

### Build Copilot Extension

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
copilot/contextshrink-vscode/contextshrink-copilot-0.1.0.vsix
```

### Install Copilot Extension

Install with VS Code CLI:

```sh
code --install-extension copilot/contextshrink-vscode/contextshrink-copilot-0.1.0.vsix
```

Or in VS Code:

```text
Extensions → ... → Install from VSIX...
```

Pick:

```text
copilot/contextshrink-vscode/contextshrink-copilot-0.1.0.vsix
```

### Use With Copilot Chat

Open the repo in VS Code.

Run Command Palette:

```text
ContextShrink: Copy Copilot Context
```

Paste into Copilot Chat, then ask your question.

For smaller clipboard payload, run:

```text
ContextShrink: Generate Copilot Context
```

Then ask Copilot:

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
