# Bonsai Context Manager

## Start

1. Install **Bonsai Context Manager** from the Visual Studio Marketplace.
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
- `Bonsai Context Manager: Generate for Request` asks what to focus on, then keeps matching modules at higher detail.
- `Bonsai Context Manager: Generate Changed` creates and opens context from the local incremental cache.
- `Bonsai Context Manager: Generate and Ask` creates context, then opens chat with instructions to read it.
- `Bonsai Context Manager: Preview Project Map` shows paths, compression levels, token counts, and savings.
- `Bonsai Context Manager: Add Agent Instructions` adds an `AGENTS.md` section telling agents to read all generated context files.
- `Bonsai Context Manager: Create Project Config` creates and opens `.bonsai.toml`.
- `Bonsai Context Manager: More Actions` opens the secondary menu with `Open Last Context`.

## Settings

The main settings control the internal generator: `maxTokens`, `level`,
`outputFile`, and `outputFormat`. Include/exclude patterns and Git-ignore
behavior control which workspace files are scanned.

The extension supports XML and JSON output. The extension handles scanning,
compression, and token budgeting itself.

The VSIX bundles the real `cl100k_base` tokenizer used by the Rust CLI default,
so token counts and token budgets use the same tokenizer in both cases.

## Project config

Put a `.bonsai.toml` file at the repository root to keep settings with the
project. The extension reads `max_tokens`, `level`, `format`, `output_file`,
`include`, `exclude`, and `respect_gitignore` from it.

```toml
max_tokens = 12000
level = 1
format = "xml"
output_file = "bonsai.xml"
include = ["src/**", "Cargo.toml"]
exclude = ["**/generated/**"]
respect_gitignore = true
```

Project settings override the VS Code defaults. Level 1 keeps more source;
level 2 keeps signatures and shapes; level 3 keeps a compact tree map.

## Agent tool

In VS Code 1.99 or newer, agent mode can use `#bonsai_generate_context` to
generate or refresh context automatically. Pass the user's repository question
or task as `request`. Bonsai derives likely files from the request, code symbols,
imports, callers, and nearby tests. The agent can optionally pass
`filePriorities`: level 1 for primary code, level 2 for supporting files, and
level 3 for compact background context. Planned paths are validated, but Bonsai
also protects request-derived files so a weak plan cannot hide them. Level 1
drops comments by default; set `includeComments` only when comments matter. The tool writes the context files,
returns their paths, and gives the agent the first context file contents. It
asks for confirmation before writing files.

If the agent does not call it automatically, enable `Generate Bonsai Context`
under Agent `Select Tools`, or reference `#bonsai_generate_context` in the
chat prompt.

To allow the tool, open Copilot Chat in **Agent** mode, click **Select Tools**,
and turn on **Generate Bonsai Context**. You can also type
`#bonsai_generate_context` in the chat prompt. VS Code asks for confirmation
before the tool writes context files.

Install it from the Visual Studio Marketplace by searching for **Bonsai Context Manager**, or run:

```sh
code --install-extension MickyBalladelli.bonsai-vscode
```

Then open the Command Palette:

```text
Bonsai Context Manager: Generate
```

The status bar shows extension version and internal engine health after
activation. After each run it also shows token count and file count. Hover it to
see the output path.

