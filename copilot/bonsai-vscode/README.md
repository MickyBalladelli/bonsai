# Bonsai

VS Code extension that generates Bonsai XML for GitHub Copilot Chat and Codex.

Install into VS Code:

```sh
"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" --install-extension copilot/bonsai-vscode/bonsai-vscode-0.1.0.vsix
```

Install into Cursor:

```sh
code --install-extension copilot/bonsai-vscode/bonsai-vscode-0.1.0.vsix
```

On some machines, `code` points to Cursor. Use the full VS Code path when you want Visual Studio Code.

Install the CLI first:

```sh
cargo install --path .
```

The extension checks `BONSAI_BIN`, then `bonsai` on `PATH`, then local release builds.

Use Command Palette:

```text
Bonsai: Generate Context
```

or:

```text
Bonsai: Generate and Ask
```

or:

```text
Bonsai: Copy Context Prompt
```

or:

```text
Bonsai: Preview Project Map
```

or:

```text
Bonsai: Copy Project Map
```
