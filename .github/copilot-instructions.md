# ContextShrink

When the user provides ContextShrink XML, treat it as compressed repository context.

If the user asks for repo-wide analysis and no ContextShrink XML is present, suggest running:

```sh
ContextShrink: Generate Copilot Context
```

or:

```sh
ContextShrink: Copy Copilot Context
```

Then use the generated XML to answer.
