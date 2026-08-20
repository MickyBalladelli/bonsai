---
description: Run the local Bonsai CLI to compress a repository into token-budgeted XML context for Claude Code. Use when the user asks for repo-wide analysis, architecture review, bug hunting across many files, onboarding to an unfamiliar codebase, summarizing a project, or explicitly asks to use Bonsai before answering.
---

# Bonsai

Use the local Bonsai binary to create compact XML repository context before broad codebase reasoning. For repo-wide prompts, run Bonsai before inspecting individual files or answering.

## Must Run For

```text
summarize this project
explain the architecture
find likely bugs across the repo
onboard me to this codebase
where should I start reading?
review the whole project
```

## Workflow

1. Prefer the helper command from the plugin `bin/` directory:

```sh
bonsai-claude <repo-path> <max-tokens> <level> <output-file> [bonsai-options...]
```

2. Default values when the user does not specify:

```text
repo-path: current workspace
max-tokens: 12000
level: 2
output-file: /tmp/bonsai.xml
```

3. Read the generated XML file before answering the user.

4. Use level choice by task:

```text
level 3: first-pass architecture map or very large repo
level 2: default repo-wide analysis
level 1: focused debugging on a smaller folder
```

5. If output is still too broad, rerun Bonsai on the most relevant subdirectory rather than asking the user to paste files.

## Commands

Default repo-wide context:

```sh
bonsai-claude . 12000 2 /tmp/bonsai.xml
```

Architecture map:

```sh
bonsai-claude . 4000 3 /tmp/bonsai.xml
```

Focused full-code pass:

```sh
bonsai-claude src 20000 1 /tmp/bonsai.xml
```

Summarize only `src`:

```sh
bonsai-claude src 12000 2 /tmp/bonsai.xml
```

Exclude generated files:

```sh
bonsai-claude . 12000 2 /tmp/bonsai.xml --exclude '**/generated/**' --exclude '**/*.generated.ts'
```

JSON output:

```sh
bonsai-claude . 12000 2 /tmp/bonsai.json --format json
```

When the user mentions a folder or glob, pass it through instead of scanning the whole repo. Prefer the path argument for a single folder and `--include` or `--exclude` for globs.

## Expected Behavior

```text
User asks: summarize this whole project
Claude runs: bonsai-claude . 12000 2 /tmp/bonsai.xml
Claude inspects: /tmp/bonsai.xml
Claude answers using the compressed repository context.
```

## Notes

- Do not start servers for this skill.
- The helper writes a file, then Claude should inspect that file with ordinary file-reading tools.
- The helper uses `BONSAI_BIN` when set, then `bonsai` from PATH, then this repo's release binary when the plugin is used from the source checkout.
