#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${BONSAI_BIN:-}"
if [[ -z "$bin" ]]; then
  if [[ -x "$repo_root/target/debug/bonsai" ]]; then
    bin="$repo_root/target/debug/bonsai"
  else
    bin="$repo_root/target/release/bonsai"
  fi
fi

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/bonsai-cli-smoke.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT

repo="$tmp_root/repo"
mkdir -p "$repo/src/deep"
cat > "$repo/Cargo.toml" <<'TOML'
[package]
name = "demo"
version = "0.1.0"
TOML
cat > "$repo/src/deep/leaf.rs" <<'RS'
fn leaf() {
    println!("leaf");
}
RS
cat > "$repo/README.md" <<'MD'
# Demo

Small repo.
MD

"$bin" "$repo" --project-map-only --output-file "$tmp_root/map.xml"
grep -Fq '<project_map>' "$tmp_root/map.xml"
if grep -Fq '<metadata' "$tmp_root/map.xml"; then
  printf 'project-map-only included metadata\n' >&2
  exit 1
fi

"$bin" "$repo" --project-map compact --project-map-only --output-file "$tmp_root/compact-map.xml"
grep -Fq '<project_map mode="compact">' "$tmp_root/compact-map.xml"
grep -Fq '<dir path="src/deep"' "$tmp_root/compact-map.xml"
grep -Fq '<entry name="leaf.rs"' "$tmp_root/compact-map.xml"
if grep -Fq 'path="src/deep/leaf.rs"' "$tmp_root/compact-map.xml"; then
  printf 'compact project map included full file path\n' >&2
  exit 1
fi

"$bin" "$repo" --no-content --output-file "$tmp_root/no-content.xml"
grep -Fq '<metadata' "$tmp_root/no-content.xml"
grep -Fq '<project_map>' "$tmp_root/no-content.xml"
if grep -Fq '<files>' "$tmp_root/no-content.xml"; then
  printf 'no-content included files\n' >&2
  exit 1
fi

"$bin" "$repo" --no-token-counts --project-map compact --directory-summaries --output-file "$tmp_root/no-token-counts.xml"
grep -Fq '<project_map mode="compact">' "$tmp_root/no-token-counts.xml"
grep -Fq '<dir path="src/deep" files="1">' "$tmp_root/no-token-counts.xml"
grep -Fq '<entry name="leaf.rs" level="2" />' "$tmp_root/no-token-counts.xml"
if grep -Fq ' tokens="' "$tmp_root/no-token-counts.xml"; then
  printf 'no-token-counts XML included token attributes\n' >&2
  exit 1
fi

"$bin" "$repo" --no-token-counts --format json --output-file "$tmp_root/no-token-counts.json"
grep -Fq '"project_map"' "$tmp_root/no-token-counts.json"
grep -Fq '"files"' "$tmp_root/no-token-counts.json"
if grep -Fq '"tokens":' "$tmp_root/no-token-counts.json"; then
  printf 'no-token-counts JSON included token fields\n' >&2
  exit 1
fi

"$bin" "$repo" --directory-summaries --output-file "$tmp_root/dirs.xml"
grep -Fq '<directory_summaries>' "$tmp_root/dirs.xml"
grep -Fq 'path="src/deep"' "$tmp_root/dirs.xml"

"$bin" "$repo" --format json --sort priority --no-content --output-file "$tmp_root/priority.json"
first_path="$(grep -o '"path":"[^"]*"' "$tmp_root/priority.json" | head -n 1)"
if [[ "$first_path" != '"path":"Cargo.toml"' ]]; then
  printf 'priority sort first path was %s\n' "$first_path" >&2
  exit 1
fi

"$bin" "$repo" --format text --output-file "$tmp_root/context.txt"
grep -Fq 'bonsai_context' "$tmp_root/context.txt"
grep -Fq 'project_map' "$tmp_root/context.txt"
grep -Fq 'files' "$tmp_root/context.txt"
grep -Fq -- '--- src/deep/leaf.rs L2 tokens=' "$tmp_root/context.txt"
if grep -Fq '<repository_context>' "$tmp_root/context.txt"; then
  printf 'text output included XML root\n' >&2
  exit 1
fi

"$bin" "$repo" --file-hashes --project-map-only --output-file "$tmp_root/hashes.xml"
grep -Eq 'hash="[0-9a-f]{64}"' "$tmp_root/hashes.xml"

cap_repo="$tmp_root/cap-repo"
mkdir -p "$cap_repo/src"
cat > "$cap_repo/src/huge.rs" <<'RS'
pub fn huge() {
    let alpha = "alpha alpha alpha alpha alpha alpha alpha";
    let beta = "beta beta beta beta beta beta beta";
    let gamma = "gamma gamma gamma gamma gamma gamma gamma";
    println!("{alpha}{beta}{gamma}");
}
RS
"$bin" "$cap_repo" --max-file-tokens 4 --project-map-only --output-file "$tmp_root/capped.xml"
grep -Fq 'path="src/huge.rs" level="3"' "$tmp_root/capped.xml"

"$bin" "$repo" --dry-run --output-file "$tmp_root/dry-run.xml" > "$tmp_root/dry-run.txt"
grep -Fq 'dry_run:' "$tmp_root/dry-run.txt"
grep -Fq '  estimated_tokens:' "$tmp_root/dry-run.txt"
grep -Fq 'selected_files:' "$tmp_root/dry-run.txt"
grep -Fq 'Cargo.toml' "$tmp_root/dry-run.txt"
if test -e "$tmp_root/dry-run.xml"; then
  printf 'dry-run wrote output file\n' >&2
  exit 1
fi

"$bin" "$repo" --dry-run --quiet --output-file "$tmp_root/dry-run-quiet.xml" > "$tmp_root/dry-run-quiet.txt"
if test -s "$tmp_root/dry-run-quiet.txt"; then
  printf 'quiet dry-run wrote stdout\n' >&2
  exit 1
fi
if test -e "$tmp_root/dry-run-quiet.xml"; then
  printf 'quiet dry-run wrote output file\n' >&2
  exit 1
fi

"$bin" "$repo" --summary --quiet --output-file "$tmp_root/quiet.xml" > "$tmp_root/quiet.txt"
if test -s "$tmp_root/quiet.txt"; then
  printf 'quiet wrote stdout\n' >&2
  exit 1
fi
test -f "$tmp_root/quiet.xml"

drop_repo="$tmp_root/drop-repo"
mkdir -p "$drop_repo/src/generated/deep"
cat > "$drop_repo/Cargo.toml" <<'TOML'
[package]
name="demo"
version="0.1.0"
TOML
for index in 1 2 3; do
  printf 'pub fn generated_%s() {}\n' "$index" > "$drop_repo/src/generated/deep/file$index.rs"
done
"$bin" "$drop_repo" --max-tokens 155 --drop-low-priority --summary --output-file "$tmp_root/drop.xml" > "$tmp_root/drop.txt"
grep -Fq '  files_included: 1' "$tmp_root/drop.txt"
grep -Fq '  files_dropped: 3' "$tmp_root/drop.txt"
grep -Fq 'path="Cargo.toml"' "$tmp_root/drop.xml"
if grep -Fq 'src/generated/deep' "$tmp_root/drop.xml"; then
  printf 'drop-low-priority kept generated files\n' >&2
  exit 1
fi

"$bin" completions bash > "$tmp_root/bonsai.bash"
"$bin" completions zsh > "$tmp_root/_bonsai"
"$bin" completions fish > "$tmp_root/bonsai.fish"
grep -Fq 'init-agent' "$tmp_root/bonsai.bash"
grep -Fq 'completions' "$tmp_root/_bonsai"
grep -Fq 'dry-run' "$tmp_root/bonsai.fish"

"$bin" --help > "$tmp_root/help.txt"
grep -Fq 'Commands:' "$tmp_root/help.txt"
grep -Fq 'init-agent' "$tmp_root/help.txt"
grep -Fq 'completions' "$tmp_root/help.txt"

"$bin" doctor "$repo" --tokenizer cl100k_base > "$tmp_root/doctor.txt"
grep -Fq 'bonsai doctor:' "$tmp_root/doctor.txt"
grep -Fq '  binary:' "$tmp_root/doctor.txt"
grep -Fq '  version:' "$tmp_root/doctor.txt"
grep -Fq '  cache_path:' "$tmp_root/doctor.txt"
grep -Fq '  cache:' "$tmp_root/doctor.txt"
grep -Fq '    size_bytes:' "$tmp_root/doctor.txt"
grep -Fq '    entries:' "$tmp_root/doctor.txt"
grep -Fq '    stale_entries:' "$tmp_root/doctor.txt"
grep -Fq '    metadata:' "$tmp_root/doctor.txt"
grep -Fq '  tokenizer: cl100k_base (ok)' "$tmp_root/doctor.txt"
grep -Fq '    .rs: tree-sitter (ok)' "$tmp_root/doctor.txt"
grep -Fq '    .md: compact (ok)' "$tmp_root/doctor.txt"

"$bin" doctor "$repo" --json --tokenizer cl100k_base > "$tmp_root/doctor.json"
grep -Fq '"binary":' "$tmp_root/doctor.json"
grep -Fq '"version":' "$tmp_root/doctor.json"
grep -Fq '"cache":' "$tmp_root/doctor.json"
grep -Fq '"stale_entries":' "$tmp_root/doctor.json"
grep -Fq '"tokenizer": {"name": "cl100k_base", "available": true' "$tmp_root/doctor.json"
grep -Fq '"extension": ".rs", "mode": "tree-sitter", "available": true' "$tmp_root/doctor.json"
if grep -Fq 'bonsai doctor:' "$tmp_root/doctor.json"; then
  printf 'doctor --json included text output\n' >&2
  exit 1
fi

if "$bin" "$repo" --max-tokens 1 --fail-over-budget --output-file "$tmp_root/over.xml" >/dev/null 2>&1; then
  printf 'fail-over-budget did not fail\n' >&2
  exit 1
fi

agents="$tmp_root/agents"
"$bin" init-agent "$agents"
test -f "$agents/AGENTS.md"
test -f "$agents/CLAUDE.md"
grep -Fq 'bonsai . --max-tokens 12000 --level 2 --output file --output-file /tmp/bonsai.xml' "$agents/AGENTS.md"
grep -Fq -- '--max-tokens' "$agents/AGENTS.md"
grep -Fq -- '--level' "$agents/AGENTS.md"
grep -Fq -- '--output file' "$agents/AGENTS.md"
grep -Fq -- '--output-file' "$agents/AGENTS.md"
grep -Fq -- '--max-tokens' "$agents/CLAUDE.md"
grep -Fq -- '--level' "$agents/CLAUDE.md"
grep -Fq -- '--output file' "$agents/CLAUDE.md"
grep -Fq -- '--output-file' "$agents/CLAUDE.md"
grep -Fq 'Do not answer from ordinary file browsing until this command succeeds' "$agents/AGENTS.md"
if grep -Fq 'target/release/bonsai' "$agents/AGENTS.md"; then
  printf 'init-agent wrote repo-local binary path\n' >&2
  exit 1
fi
if "$bin" init-agent "$agents" >/dev/null 2>&1; then
  printf 'init-agent overwrote without --force\n' >&2
  exit 1
fi
"$bin" init-agent "$agents" --force >/dev/null

partial_agents="$tmp_root/partial-agents"
mkdir -p "$partial_agents"
printf 'custom claude instructions\n' > "$partial_agents/CLAUDE.md"
if "$bin" init-agent "$partial_agents" >/dev/null 2>&1; then
  printf 'init-agent allowed partial overwrite\n' >&2
  exit 1
fi
if test -f "$partial_agents/AGENTS.md"; then
  printf 'init-agent wrote AGENTS.md before detecting existing CLAUDE.md\n' >&2
  exit 1
fi
"$bin" init-agent "$partial_agents" --force >/dev/null
test -f "$partial_agents/AGENTS.md"

printf 'cli output smoke passed\n'
