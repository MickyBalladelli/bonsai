# TODO

## Project review follow-up

Prioritize context quality, budget enforcement, and engine consistency before adding more features.

### Preserve useful context

- [x] Make the default compression level preserve important data across the CLI and extension, including implementation logic, types, configuration values, and meaningful comments. Do not silently downgrade or truncate important content to meet a token budget; report when it cannot fit and require explicit opt-in for lossy compression. Changing the default to level 1 alone is insufficient while automatic budget downgrades can still discard that content.
- [x] Fix file ranking so implementation files keep useful detail before lockfiles and dependency noise. The reviewed `bonsai.xml` gave `package-lock.json` 1,467 tokens but `internalGenerator.ts` only 18.
- [x] Improve request-aware selection: preserve code relevant to the question and its supporting dependencies while shrinking background files.
- [x] Make severe truncation and missing implementation detail clear to agents so compressed context does not imply enough evidence for a full review.

### Enforce token budgets

- [x] Fix the extension's `fitBudget` loop so the 200-attempt limit cannot silently return over-budget output.
- [x] Keep considering other compressible files when the selected candidate reaches its minimum size.
- [x] Verify the final emitted token count and report clearly when the requested budget cannot be met, with consistent CLI and extension behavior.

### Align the engines

- [x] Choose a shared compression core or define and maintain explicit behavior parity between the Rust CLI and TypeScript extension.
- [x] Replace fragile extension body-scanning rules with syntax-aware handling, including multiline function signatures.
- [x] Parse `.tsx` with the Tree-sitter TSX grammar instead of the TypeScript grammar.
- [x] Document remaining engine differences in parsing, selection, configuration, and budget handling.

### Keep generated output out of input

- [x] Exclude Bonsai's configured output and numbered chunks from source scans, including unignored `bonsai.json` files.
- [x] Retire stale numbered chunks when regeneration produces fewer files, without touching unrelated files.

### Prove quality and product value

- [ ] Add the existing extension smoke tests to CI; the current extension job only compiles and packages.
- [ ] Build a real-task evaluation covering answer correctness, important files retained, total tokens including follow-up reads, and completion time.
- [ ] Compare Bonsai against direct repository reading, Repomix, and Aider repository maps on equivalent tasks and budgets.
- [ ] Demonstrate that token savings preserve answer quality rather than merely producing smaller output.
- [ ] Define and document Bonsai's distinct value around request-aware context selection; repository packing and syntax compression alone already exist elsewhere.
- [ ] Use measured quality and user demand to decide whether to pursue a paid product; keep improving the useful personal and open-source tool in the meantime.

## Usability review

Current ease: easy for the first CLI run, medium for normal use, hard for advanced use.
`bonsai .` is a good start, but users must understand installation, output files,
token budgets, compression levels, filters, cache behavior, and several change modes.

## Make the first run simple

- [x] Print a clear success message after `bonsai .`: output path, files included, and token count.
- [x] Add one documented install command per platform, with checksum verification and no manual binary-path guessing.
- [x] Add a first-run setup command that checks the binary, tokenizer, parsers, clipboard, and output path.
- [x] Make output-file parent directories automatically, and warn before silently replacing an existing output file.
- [x] Add a short beginner guide with three copy-paste flows: full context, changed context, and project map.

## Reduce choice overload

- [x] Add simple presets or aliases for the common flows: full context, changed context, map only, and prompt to clipboard.
- [x] Add a `.bonsai.toml` or `bonsai config` flow so users do not repeat long flag sets.
- [x] Keep advanced flags, but group them in help and docs by budget, selection, output, and incremental mode.
- [x] Explain compression levels with real examples in `--help`, not only numeric names.
- [x] Make incremental modes easier to understand by presenting one clear changed-context workflow and documenting cache baseline rules.
- [x] Choose safer, consistent defaults for budget, level, output path, and output format across the CLI, VS Code extension, and generated agent files.

## Simplify VS Code

- [x] Show a setup wizard when Bonsai is missing, with an actionable install or binary-path message.
- [x] Bundle or download the matching Bonsai binary instead of relying on PATH and hard-coded developer paths.
- [x] Keep three primary commands visible: Generate, Generate changed, and Generate and ask. Move map and diagnostics into secondary actions.
- [x] Support multi-root workspaces, or clearly show which workspace folder will be used before generation.
- [x] Expose the useful core settings first. Put rare CLI controls behind an advanced section instead of trying to mirror every flag.
- [x] Align extension capabilities with the CLI, or clearly document unsupported features such as text output and advanced budget controls.
- [x] Show process errors and warnings in a Bonsai output channel, not only in a generic failed-command message.

## Make docs and maintenance simpler

- [x] Add a decision table: goal, command, output, and when to use it.
- [x] Add one canonical option reference generated from the CLI schema so README, VS Code settings, and help cannot drift.
- [x] Document cache location, cache invalidation, `--incremental`, `--incremental-base`, and `--changed-since` with one small example each.
- [x] Add a supported-file table showing parser-backed files versus compact fallback files.
- [x] Make `init-agent` let users choose which agent file to create and generate shorter, configurable instructions.
- [x] Add a visible version and health check to the CLI and VS Code UI.
