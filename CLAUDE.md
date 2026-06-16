For repo-wide analysis, first run Bonsai. Do not answer from ordinary file browsing until this command succeeds:

```sh
target/release/bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
```

Then inspect `/tmp/bonsai.xml` before answering.

Repo-wide analysis includes architecture review, full project summary, broad bug hunting, onboarding, and questions that need many files.

Expected behavior example:

```text
User asks: summarize this whole project
Claude runs: target/release/bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml
Claude inspects: /tmp/bonsai.xml
Claude answers from that context.
```
