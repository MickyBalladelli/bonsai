# Bonsai Claude Code Plugin

Claude Code plugin for generating token-budgeted Bonsai XML before broad repo analysis.

Install the CLI first:

```sh
cargo install --path .
```

The helper checks `BONSAI_BIN`, then `bonsai` on `PATH`, then this repo's release binary.

Test from the repo root:

```sh
claude --plugin-dir ./claude/bonsai
```

Use in Claude Code:

```text
/bonsai:bonsai
```

The skill writes `/tmp/bonsai.xml`, then Claude reads it before answering.

Expected behavior:

```text
Ask: summarize this whole project
See: Bonsai command execute
Inspect: /tmp/bonsai.xml
Answer: summary uses compressed repository context
```

Marketplace install from this repo root:

```sh
claude plugin marketplace add .
```

Then install in Claude Code:

```text
/plugin install bonsai@bonsai
```

Validate before publishing:

```sh
claude plugin validate .
```

Examples:

```sh
claude/bonsai/bin/bonsai-claude src 12000 2 /tmp/bonsai-src.xml
claude/bonsai/bin/bonsai-claude . 12000 2 /tmp/bonsai.xml --exclude '**/generated/**'
claude/bonsai/bin/bonsai-claude . 12000 2 /tmp/bonsai.json --format json
```

Release checklist:

```text
cargo test
cargo build --release
claude plugin validate .
claude/bonsai/bin/bonsai-claude . 12000 2 /tmp/bonsai.xml
bump claude/bonsai/.claude-plugin/plugin.json version for pinned releases
```
