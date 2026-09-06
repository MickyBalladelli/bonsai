# Bonsai evaluation

How we measure that Bonsai's token savings preserve answer quality instead of
merely producing smaller output.

## What the harness measures

`tests/eval/run.py` runs the CLI on fixed tasks and reports deterministic
proxies (stdlib only, no network, no model calls):

```sh
BONSAI_BIN=target/release/bonsai python3 tests/eval/run.py
```

| Metric | Meaning |
| --- | --- |
| recall | Fraction of hand-labeled task-relevant files present in the context |
| L1 recall | Fraction of those files kept at full detail (level 1) |
| raw / shrunk | Full-source tokens vs emitted tokens from `--stats` |
| follow-up* | Estimated tokens (chars/4) to read missing relevant files separately |
| seconds | Wall-clock CLI time |
| warnings | Count of fidelity warnings embedded in the output |

The tasks cover a focused bug-fix repo at the default lossless level, the
same repo widened under a tight level-2 budget, and a real question against
Bonsai's own `src/` ("where is stale chunk retirement implemented?",
relevant files `src/walker.rs` and `src/main.rs`).

## Latest results (2026-09-06, debug binary)

| task | seconds | files | raw | shrunk | saved | recall | L1 recall | follow-up* | warnings |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| auth-fix | 0.67 | 7 | 618 | 618 | 0.0% | 1.00 | 1.00 | 0 | 0 |
| auth-fix-tight-budget | 0.68 | 19 | 1401 | 1499 | 0.0% | 1.00 | 1.00 | 0 | 1 |
| bonsai-self-chunks | 2.91 | 7 | 83778 | 9728 | 88.4% | 1.00 | 0.00 | 0 | 1 |

Reading: the default level keeps every relevant file byte-for-byte
(`shrunk == raw`, recall 1.00, no warnings), so savings at level 1 come from
*selection and ranking*, never from content loss. Under an explicit lossy
budget the harness still finds all relevant files, and the output says so
via warnings instead of silently implying full evidence. On the real-repo
task, request-aware selection kept both relevant files while cutting
83,778 tokens to 9,728. (On tiny fixtures, fixed metadata overhead can make
`shrunk` exceed `raw`; the percentage is only meaningful past a few thousand
tokens.)

## Answer-correctness grading (manual rubric)

Deterministic proxies cannot score answers. Grade sampled tasks by hand (or
with an LLM judge) on:

1. The answer names the correct files and symbols, with no hallucinated paths.
2. No relevant file outside the context contradicts the answer.
3. When warnings are present, the answer hedges exactly as the warnings require
   (tree-map summaries and mid-content cuts are not behavior evidence).

Record the verdict next to the harness row before claiming a quality win.

## Baseline comparisons (protocol)

Compare on the same tasks and the same token budgets:

- **Direct repository reading**: cost is the full-source token count of every
  file the agent would open. The harness `raw` column is this number for the
  scanned set; add follow-up reads for files opened after the first pass.
- **Repomix**: run `repomix` on the fixture repo, then score the same recall
  and correctness rubric on its packed output.
- **Aider repository maps**: run Aider's map on the fixture repo and score
  whether the map alone lets the judge answer without opening files.

No Repomix/Aider numbers are recorded here yet; contributors can add rows to
the table above following this protocol. Do not compare wall-clock time
across machines — compare tokens and rubric verdicts.
