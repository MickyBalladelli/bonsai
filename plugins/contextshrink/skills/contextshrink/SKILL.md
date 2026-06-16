---
name: contextshrink
description: Run the local ContextShrink CLI to compress a repository into token-budgeted XML context for Codex. Use when the user asks for repo-wide analysis, architecture review, bug hunting across many files, onboarding to an unfamiliar codebase, summarizing a project, or explicitly asks to use ContextShrink before answering.
---

# ContextShrink

Use the local ContextShrink binary to create compact XML repository context before broad codebase reasoning.

## Workflow

1. Prefer the helper script:

```sh
scripts/run_contextshrink.sh <repo-path> <max-tokens> <level> <output-file>
```

2. Default values when the user does not specify:

```text
repo-path: current workspace
max-tokens: 12000
level: 2
output-file: /tmp/contextshrink.xml
```

3. Read the generated XML file before answering the user.

4. Use level choice by task:

```text
level 3: first-pass architecture map or very large repo
level 2: default repo-wide analysis
level 1: focused debugging on a smaller folder
```

5. If output is still too broad, rerun ContextShrink on the most relevant subdirectory rather than asking the user to paste files.

## Commands

Default repo-wide context:

```sh
scripts/run_contextshrink.sh . 12000 2 /tmp/contextshrink.xml
```

Architecture map:

```sh
scripts/run_contextshrink.sh . 4000 3 /tmp/contextshrink.xml
```

Focused full-code pass:

```sh
scripts/run_contextshrink.sh src 20000 1 /tmp/contextshrink.xml
```

## Notes

- Do not start servers for this skill.
- The helper writes a file, then Codex should inspect that file with ordinary file-reading tools.
- The helper uses `CONTEXTSHRINK_BIN` when set, then `contextshrink` from PATH, then a repo-local release binary when available.
