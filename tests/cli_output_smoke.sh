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

"$bin" "$repo" --no-content --output-file "$tmp_root/no-content.xml"
grep -Fq '<metadata' "$tmp_root/no-content.xml"
grep -Fq '<project_map>' "$tmp_root/no-content.xml"
if grep -Fq '<files>' "$tmp_root/no-content.xml"; then
  printf 'no-content included files\n' >&2
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

"$bin" doctor "$repo" --tokenizer cl100k_base > "$tmp_root/doctor.txt"
grep -Fq 'bonsai doctor:' "$tmp_root/doctor.txt"
grep -Fq '  binary:' "$tmp_root/doctor.txt"
grep -Fq '  version:' "$tmp_root/doctor.txt"
grep -Fq '  cache_path:' "$tmp_root/doctor.txt"
grep -Fq '  tokenizer: cl100k_base (ok)' "$tmp_root/doctor.txt"
grep -Fq '    .rs: tree-sitter (ok)' "$tmp_root/doctor.txt"
grep -Fq '    .md: compact (ok)' "$tmp_root/doctor.txt"

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
