# What Bonsai is for

Repository packing (Repomix) and syntax compression already exist elsewhere.
Bonsai's distinct value is **request-aware context selection with honest
budgets**: given a task, it keeps the files that matter at full detail,
shrinks the background first, and says clearly when the result is incomplete.

## The mechanism, concretely

- **Request focus, not just filters.** `--focus` (CLI) and the `request` /
  `filePriorities` tool inputs (extension) derive search terms from the task.
  `src/focus.rs` scores files by term matches, declared symbols, imports,
  callers, and nearby tests, then follows the dependency graph — so
  `session.ts` keeps full detail for a login question because it imports the
  matched `auth.ts`, not because a glob said so.
- **Ranking protects signal.** Implementation files keep detail before
  lockfiles, minified bundles, and generated files, which are demoted first
  under pressure. Explicit agent priorities boost detail further but never
  delete request-derived candidates.
- **Lossless by default, lossy by opt-in.** Level 1 preserves implementation
  logic, types, configuration values, and meaningful comments, and *fails*
  instead of silently downgrading when the budget cannot fit. Levels 2
  (skeleton) and 3 (tree map) are explicit opt-ins, and their output carries
  a warnings block naming exactly what was degraded.
- **Reported numbers describe emitted bytes.** The final token count is a
  recount of the exact output, and over-budget wording is embedded in the
  artifact itself — agents downstream cannot mistake a truncated context for
  full evidence.
- **Two engines, one contract.** The Rust CLI is the canonical batch engine;
  the VS Code extension ships a self-contained TypeScript engine (see
  `docs/engine-parity.md`) so generation works with no binary and no setup.

## What Bonsai is not

- Not a whole-repo packer: packing everything maximizes tokens, not answers.
- Not a static map: maps show structure; Bonsai ships the relevant bodies.
- Not a paid product today: that decision waits on measured quality from
  `docs/evaluation.md` and real user demand. Until then it stays a useful
  personal and open-source tool.
