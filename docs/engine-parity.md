# Engine parity: Rust CLI and TypeScript extension

Decision: keep two engines with an explicit parity contract. The Rust CLI
(`src/`) is the canonical batch engine. The VS Code extension
(`copilot/bonsai-vscode/src/internalGenerator.ts`) is a self-contained
TypeScript engine so Generate works with no binary, no PATH setup, and no
download. A shared core (for example Rust compiled to WASM) stays an option,
but it is not required to close the current consistency gaps; the contract
below plus conformance tests is the maintained parity mechanism.

## Shared compression contract

- Levels: L1 full source (default, lossless; fails instead of silently
  downgrading), L2 skeleton (signatures and structure, bodies become
  `{ ... }` or equivalent), L3 tree map (names only).
- Level 1 preserves implementation logic, types, configuration values, and
  meaningful comments.
- File ranking protects implementation files before lockfiles and dependency
  noise, then entry points and manifests, then config, then the rest.
- Request-aware selection (`--focus` / `focus` / `filePriorities`) keeps
  task-relevant files and their dependencies at higher detail and shrinks
  background files first.
- Budget fitting downgrades or truncates the lowest-value detail first and
  never blocks on a file already at its minimum.
- Final verification recounts the exact emitted bytes and reports
  `over budget even after maximum compression; treat this context as
  incomplete` when the budget cannot be met. Reported counts, embedded
  warnings, and UI/summary notices must agree.
- Fidelity warnings name tree-map summaries, mid-content cuts ending in
  `...`, omitted files, and over-budget output.

## What changed for this task

- CLI parses `.tsx` with the Tree-sitter TSX grammar
  (`tree_sitter_typescript::language_tsx`) instead of the plain TypeScript
  grammar. `.ts` still uses `language_typescript`. Skeleton, tree-map, and
  symbol rules for TSX match TypeScript.
- Extension skeleton scanning is syntax-aware: braces inside strings and
  comments no longer open bodies, and the signature check uses the current
  statement (up to 2000 chars back, strings/comments blanked, whitespace
  collapsed) instead of only the current line. Multiline signatures such as
  `function foo(\n a: string\n): Result {` now strip correctly.

## Remaining differences

### Parsing

- CLI uses Tree-sitter grammars per language (JS, TS, TSX, Python, Rust, Go,
  Java, C#, Swift, Kotlin, C, C++). Extension uses comment stripping plus
  brace/paren scanning plus line-pattern tree maps, with no Tree-sitter
  dependency. Expect small skeleton differences on exotic syntax; the
  contract above is what must hold, not byte-identical output.
- Objective-C (`.m`, `.mm`), Vue/Svelte/Astro/HTML, Markdown, and JSON/YAML/
  TOML use compact fallback handling in both engines, but the line caps
  differ slightly (`MAX_TEXT_LINES` 180 vs 160-180 paths in CLI helpers).
- TSX JSX elements are declarations-neutral in both engines: component
  function signatures are kept, JSX bodies are stripped at L2 and omitted
  at L3.

### Selection

- Both rank lockfiles lowest and protect manifests, entry points, and
  implementation files, with task relevance boosting detail.
- CLI request focus lives in `src/focus.rs` with symbol/import/test
  proximity. Extension mirrors it with `extractFocusTerms`,
  `calculateTaskRelevance`, dependency graph distance, and explicit
  `filePriorities` from the agent tool. Scoring constants are not identical;
  ordering guarantees (relevant first, lockfiles first to downgrade) are.
- CLI can drop lowest-priority files with `--drop-low-priority`. The
  extension never drops files; it downgrades then truncates toward an
  8-token floor.

### Configuration

- Extension settings: `maxTokens`, `level`, `outputFile`, `outputFormat`
  (`xml`|`json`), `include`, `exclude`, `respectGitignore`, plus
  `.bonsai.toml` project config.
- CLI additionally supports `--tokenizer` families (o200k, cl100k, p50k,
  r50k), `--max-file-tokens`, `--format text`, `--project-map-only`,
  `--map-only-under`, `--sort`, `--directory-summaries`, `--file-hashes`,
  `--no-token-counts`, `--dry-run`, `--fail-over-budget`,
  `--drop-low-priority`, `--fail-on-empty`, `--quiet`, `--stats`,
  `--prompt`/`--ask-template`, cache and incremental modes
  (`--incremental`, `--incremental-base`, `--changed-since`), and shell
  completions. These have no extension equivalent and are intentionally
  hidden behind CLI-only advanced use.

### Budget handling and output

- Tokenizers differ: CLI counts with `tiktoken-rs` and the selected family.
  Extension counts with `js-tiktoken` `cl100k_base` only. Counts agree for
  the default tokenizer and may differ for other CLI families.
- CLI writes one output file (plus stdout/summary/stats modes). Extension
  splits output into numbered chunks under 10 MB each and recounts the sum
  across all chunks.
- Over-budget wording differs by surface only: CLI says `max-tokens`,
  extension says `max_tokens`. Both carry token count, budget, and the
  incomplete-evidence notice.

## Conformance

- Rust: `cargo test` parser cases cover TS skeleton, TSX component
  skeleton, Rust/Python skeletons, and import collapsing.
- Extension: `npm test` runs `bonsai.smoke.test` and
  `internalGenerator.smoke.test`, covering budget convergence, warning
  fidelity, dependency-aware levels, and artifact/report agreement.
- When changing either engine, update this file and the corresponding
  conformance test in the same change.
