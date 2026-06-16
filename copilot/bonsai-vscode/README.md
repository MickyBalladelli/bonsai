# Bonsai VS Code

VS Code extension for generating Bonsai repo context for Copilot Chat, ChatGPT, and Codex.

Install the CLI first:

```sh
cargo install --path .
```

Install the VSIX:

```sh
code --install-extension copilot/bonsai-vscode/bonsai-vscode-0.2.1.vsix
```

Run Command Palette:

```text
Bonsai: Generate and Ask
```

If chat does not open automatically, paste the copied prompt into your chat.

Other commands:

```text
Bonsai: Generate Context
Bonsai: Copy Context Prompt
Bonsai: Copy Project Map
Bonsai: Preview Project Map
Bonsai: Open Last Context
```

The extension checks `BONSAI_BIN`, then `bonsai` on `PATH`, then local release builds.

On some machines, `code` points to Cursor. Use the full VS Code path when you want Visual Studio Code:

```sh
"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" --install-extension copilot/bonsai-vscode/bonsai-vscode-0.2.1.vsix
```
