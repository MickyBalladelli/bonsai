# Bonsai Context Manager

## Start

1. Install the VSIX.
2. Open one or more repository folders in VS Code.
3. Run `Bonsai Context Manager: Generate`.

The extension is self-contained. It does not download, install, or execute a
separate Bonsai binary.

Generated context is written to files. Use `Bonsai Context Manager: Add Agent
Instructions` to add an `AGENTS.md` section telling agents to read every
generated context file.

[Bonsai on GitHub](https://github.com/MickyBalladelli/bonsai)

## Commands

- `Bonsai Context Manager: Generate` creates and opens the context file.
- `Bonsai Context Manager: Generate Changed` creates and opens context from the local incremental cache.
- `Bonsai Context Manager: Generate and Ask` creates context, then opens chat with instructions to read it.
- `Bonsai Context Manager: Preview Project Map` shows paths, compression levels, token counts, and savings.
- `Bonsai Context Manager: Add Agent Instructions` adds an `AGENTS.md` section telling agents to read all generated context files.
- `Bonsai Context Manager: More Actions` opens the secondary menu with `Open Last Context`.

## Settings

The main settings control the internal generator: `maxTokens`, `level`,
`outputFile`, and `outputFormat`. Include/exclude patterns and Git-ignore
behavior control which workspace files are scanned.

The extension supports XML and JSON output. The extension handles scanning,
compression, and token budgeting itself.

## Agent tool

In VS Code 1.99 or newer, agent mode can use `#bonsai_generate_context` to
generate or refresh context automatically. The tool writes the context files,
returns their paths, and gives the agent the first context file contents. It
asks for confirmation before writing files.

If the agent does not call it automatically, enable `Generate Bonsai Context`
under Agent `Select Tools`, or reference `#bonsai_generate_context` in the
chat prompt.

To allow the tool, open Copilot Chat in **Agent** mode, click **Select Tools**,
and turn on **Generate Bonsai Context**. You can also type
`#bonsai_generate_context` in the chat prompt. VS Code asks for confirmation
before the tool writes context files.

Install the VSIX:

```sh
code --install-extension copilot/bonsai-vscode/bonsai-vscode-*.vsix
```

Run Command Palette:

```text
Bonsai Context Manager: Generate
```

The status bar shows extension version and internal engine health after
activation. After each run it also shows token count and file count. Hover it to
see the output path.

On some machines, `code` points to Cursor. Use the full VS Code path when you want Visual Studio Code:

```sh
"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" --install-extension copilot/bonsai-vscode/bonsai-vscode-*.vsix
```
