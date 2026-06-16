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

if "$bin" "$repo" --max-tokens 1 --fail-over-budget --output-file "$tmp_root/over.xml" >/dev/null 2>&1; then
  printf 'fail-over-budget did not fail\n' >&2
  exit 1
fi

agents="$tmp_root/agents"
"$bin" init-agent "$agents"
test -f "$agents/AGENTS.md"
test -f "$agents/CLAUDE.md"
if "$bin" init-agent "$agents" >/dev/null 2>&1; then
  printf 'init-agent overwrote without --force\n' >&2
  exit 1
fi
"$bin" init-agent "$agents" --force >/dev/null

printf 'cli output smoke passed\n'
