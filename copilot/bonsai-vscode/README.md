# Bonsai VS Code

## Start

1. Install the VSIX.
2. Open one or more repository folders in VS Code.
3. Run `Bonsai: Generate`.
4. If the binary is missing, choose `Download Bonsai` in the setup prompt.

The extension stores its downloaded, SHA-256-verified binary in VS Code global
storage. It supports macOS Apple Silicon and Linux x64 downloads. On another
platform, install Bonsai separately and set `bonsai.binaryPath`.

## Primary commands

- `Bonsai: Generate` creates the context file and copies a prompt.
- `Bonsai: Generate Changed` creates context from the local incremental cache.
- `Bonsai: Generate and Ask` opens chat with the generated context.
- `Bonsai: More Actions` contains project-map actions, last-context opening, and setup.

## Settings

The main settings map to CLI options: `maxTokens` to `--max-tokens`, `level` to
`--level`, `outputFile` to `--output-file`, and `outputFormat` to `--format`.
Include/exclude patterns and Git-ignore behavior map to `--include`, `--exclude`,
and `--no-respect-gitignore`. Binary path is under `Bonsai: Advanced`.

The extension supports XML and JSON output. Text output and the CLI's advanced
budget, sorting, file-cap, hash, and incremental-base controls remain CLI-only.
Use the CLI when those controls are needed.

Build the CLI from this checkout if you do not want the setup prompt to download
the binary:

```sh
cargo install --path .
```

Install the VSIX:

```sh
code --install-extension copilot/bonsai-vscode/bonsai-vscode-*.vsix
```

Run Command Palette:

```text
Bonsai: Generate
```

If chat does not open automatically, paste the copied prompt into your chat.

The status bar shows extension version and binary health after activation. After
each run it also shows token count and file count. Hover it to see the binary and
output path.

The CLI is the full option reference. From the repository root, run
`sh scripts/generate-cli-reference.sh` after changing CLI flags.

On some machines, `code` points to Cursor. Use the full VS Code path when you want Visual Studio Code:

```sh
"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" --install-extension copilot/bonsai-vscode/bonsai-vscode-*.vsix
```
