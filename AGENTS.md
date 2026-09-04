# Mandatory Repository Preflight

Before answering any broad question, performing a repository-wide review or making cross-cutting changes, you MUST complete the preflight:

1. Check the repository root for `bonsai.xml`, `bonsai.json`, or numbered files such as `bonsai-2.xml` and `bonsai-2.json`.
2. If any exist, read all of them before continuing.
3. If non exist, invoke the `Generate Bonsai Context` tool.
4. Do not begin the repository-wide task until the context has been read or generated.
5. IF thegeneration tool is unavailable, stop and tell the user that Bonsai context must be generated or the tool enabled.
This preflight is required for broad repository tasks. It may be skipped for barrow questions limited to a named file or symbol. 


For repo-wide analysis, first run Bonsai. This is required for full project summaries, architecture review, onboarding, broad bug hunting, and any question that needs many files. Do not answer from ordinary file browsing until this command succeeds:

```sh
bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
```

Then inspect `/tmp/bonsai.xml` before answering.

Expected behavior example:

```text
User asks: summarize this whole project
You run: bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
You inspect: /tmp/bonsai.xml
Then answer from that context.
```

<!-- BONSAI_CONTEXT_MANAGER_START -->
# Mandatory Repository Preflight

Before answering any broad question, performing a repository-wide review or
making cross-cutting changes, you MUST complete the preflight:

1. Check the repository root for `bonsai.xml`, `bonsai.json`, or numbered files
   such as `bonsai-2.xml` and `bonsai-2.json`.
2. If any exist, read all of them before continuing.
3. If none exist, invoke the `Generate Bonsai Context` tool
   (`#bonsai_generate_context`).
4. Do not begin the repository-wide task until the context has been read or
   generated.
5. If the generation tool is unavailable, stop and tell the user that Bonsai
   context must be generated or the tool enabled.

This preflight is required for broad repository tasks. It may be skipped for
narrow questions limited to a named file or symbol.
<!-- BONSAI_CONTEXT_MANAGER_END -->
