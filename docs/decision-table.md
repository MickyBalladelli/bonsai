# Bonsai decision table

Pick the row that matches the job:

| Goal | Command | Output | When to use it |
| --- | --- | --- | --- |
| Understand a repository | `bonsai .` | `bonsai.xml` | First run, architecture, onboarding, or broad review |
| Refresh only local changes | `bonsai . --preset changed` | `bonsai.xml` | After one normal run created the local baseline |
| Compare a branch or commit | `bonsai . --preset changed --changed-since main --output-file bonsai-changes.xml` | `bonsai-changes.xml` | Review work against a Git ref, including untracked files and deletions |
| See the project shape | `bonsai . --preset map --output-file bonsai-map.xml` | `bonsai-map.xml` | Need paths and structure before spending tokens on file bodies |
| Make a paste-ready prompt | `bonsai . --preset prompt --output clipboard` | Clipboard | Send context directly to an LLM |
| Check selection before writing | `bonsai . --dry-run` | Terminal only | Verify includes, excludes, and estimated tokens |
| Check installation health | `bonsai doctor` | Terminal or JSON | Diagnose the binary, tokenizer, parsers, and cache |
| Add agent instructions | `bonsai init-agent --files agents` | `AGENTS.md` | Make Codex-style agents run Bonsai first |

In VS Code, start with `Bonsai: Generate`. Use `Bonsai: Generate Changed` for
the local cache workflow and `Bonsai: Generate and Ask` when chat should open.
