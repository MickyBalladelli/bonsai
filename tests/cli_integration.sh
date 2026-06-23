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

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/bonsai-cli-integration.XXXXXX")"
trap 'chmod -R u+rwX "$tmp_root" 2>/dev/null || true; rm -rf "$tmp_root"' EXIT

normalize_xml() {
  sed -E 's/generated_at="[0-9]+"/generated_at="TIMESTAMP"/; s#repo_root="[^"]+"#repo_root="/REPO"#' "$1"
}

normalize_json() {
  sed -E 's/"generated_at":"[0-9]+"/"generated_at":"TIMESTAMP"/; s#"repo_root":"[^"]+"#"repo_root":"/REPO"#' "$1"
}

make_golden_repo() {
  local repo="$1"
  mkdir -p "$repo/src"
  cat > "$repo/Cargo.toml" <<'TOML'
[package]
name = "demo"
version = "0.1.0"
TOML
  cat > "$repo/src/lib.rs" <<'RS'
pub fn greet(name: &str) -> String {
    format!("hello {name}")
}
RS
}

make_flag_repo() {
  local repo="$1"
  mkdir -p "$repo/src" "$repo/tests"
  mkdir -p "$repo/.git"
  cat > "$repo/.gitignore" <<'GITIGNORE'
ignored.rs
GITIGNORE
  cat > "$repo/src/lib.rs" <<'RS'
pub fn keep() {}
RS
  cat > "$repo/src/skip.rs" <<'RS'
pub fn skip() {}
RS
  cat > "$repo/tests/test.rs" <<'RS'
pub fn test_only() {}
RS
  cat > "$repo/ignored.rs" <<'RS'
pub fn ignored() {}
RS
}

golden_repo="$tmp_root/golden-repo"
make_golden_repo "$golden_repo"
"$bin" "$golden_repo" --max-tokens 12000 --level 2 --output-file "$tmp_root/context.xml"
normalize_xml "$tmp_root/context.xml" > "$tmp_root/context.normalized.xml"
diff -u "$repo_root/tests/golden/context.xml" "$tmp_root/context.normalized.xml"

"$bin" "$golden_repo" --max-tokens 12000 --level 2 --format json --output-file "$tmp_root/context.json"
normalize_json "$tmp_root/context.json" > "$tmp_root/context.normalized.json"
diff -u "$repo_root/tests/golden/context.json" "$tmp_root/context.normalized.json"

flag_repo="$tmp_root/flag-repo"
make_flag_repo "$flag_repo"
"$bin" "$flag_repo" --include 'src/**' --exclude '**/skip.rs' --print-files --output-file "$tmp_root/filtered.xml" > "$tmp_root/filtered.txt"
grep -Fxq 'src/lib.rs' "$tmp_root/filtered.txt"
if grep -Eq 'skip|tests/' "$tmp_root/filtered.txt"; then
  printf 'include/exclude selected wrong files\n' >&2
  exit 1
fi

"$bin" "$flag_repo" --print-files --output-file "$tmp_root/respect.xml" > "$tmp_root/respect.txt"
if grep -Fxq 'ignored.rs' "$tmp_root/respect.txt"; then
  printf 'gitignore was not respected\n' >&2
  exit 1
fi

"$bin" "$flag_repo" --no-respect-gitignore --print-files --output-file "$tmp_root/no-respect.xml" > "$tmp_root/no-respect.txt"
grep -Fxq 'ignored.rs' "$tmp_root/no-respect.txt"

"$bin" "$golden_repo" --prompt --output-file "$tmp_root/prompt.txt"
grep -Fq 'Use this repo context to explain the architecture' "$tmp_root/prompt.txt"
grep -Fq '<context>' "$tmp_root/prompt.txt"

"$bin" "$golden_repo" --ask-template 'Find risks.' --output-file "$tmp_root/ask.txt"
grep -Fq 'Find risks.' "$tmp_root/ask.txt"
grep -Fq '<context>' "$tmp_root/ask.txt"

incremental_repo="$tmp_root/incremental-repo"
make_golden_repo "$incremental_repo"
"$bin" "$incremental_repo" --incremental --output-file "$tmp_root/incremental-first.xml"
grep -Fq 'path="Cargo.toml"' "$tmp_root/incremental-first.xml"
grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-first.xml"

"$bin" "$incremental_repo" --incremental --output-file "$tmp_root/incremental-second.xml"
if grep -Fq 'path="Cargo.toml"' "$tmp_root/incremental-second.xml" || grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-second.xml"; then
  printf 'unchanged incremental run included files\n' >&2
  exit 1
fi

cat >> "$incremental_repo/src/lib.rs" <<'RS'

pub fn farewell(name: &str) -> String {
    format!("bye {name}")
}
RS
"$bin" "$incremental_repo" --incremental --output-file "$tmp_root/incremental-third.xml"
grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-third.xml"
if grep -Fq 'path="Cargo.toml"' "$tmp_root/incremental-third.xml"; then
  printf 'incremental run included unchanged file\n' >&2
  exit 1
fi

incremental_json_repo="$tmp_root/incremental-json-repo"
make_golden_repo "$incremental_json_repo"
"$bin" "$incremental_json_repo" --max-tokens 12000 --level 2 --format json --output-file "$tmp_root/incremental-json-seed.json"
cat >> "$incremental_json_repo/src/lib.rs" <<'RS'

pub fn farewell(name: &str) -> String {
    format!("bye {name}")
}
RS
"$bin" "$incremental_json_repo" --max-tokens 12000 --level 2 --format json --incremental --output-file "$tmp_root/incremental-context.json"
normalize_json "$tmp_root/incremental-context.json" > "$tmp_root/incremental-context.normalized.json"
diff -u "$repo_root/tests/golden/incremental-context.json" "$tmp_root/incremental-context.normalized.json"

"$bin" cache clear "$incremental_repo" > "$tmp_root/cache-clear.txt"
grep -Fq 'cleared cache for' "$tmp_root/cache-clear.txt"
"$bin" "$incremental_repo" --incremental --output-file "$tmp_root/incremental-after-clear.xml"
grep -Fq 'path="Cargo.toml"' "$tmp_root/incremental-after-clear.xml"
grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-after-clear.xml"
rm "$incremental_repo/Cargo.toml"
"$bin" "$incremental_repo" --incremental --incremental-summary --output-file "$tmp_root/incremental-local-delete.xml" > "$tmp_root/incremental-local-delete.txt"
grep -Fq '  skipped: 1' "$tmp_root/incremental-local-delete.txt"
grep -Fq '  deleted: 1' "$tmp_root/incremental-local-delete.txt"
grep -Fq '<deleted_files>' "$tmp_root/incremental-local-delete.xml"
grep -Fq '<deleted path="Cargo.toml" />' "$tmp_root/incremental-local-delete.xml"

incremental_options="$tmp_root/incremental-options"
make_golden_repo "$incremental_options"
"$bin" "$incremental_options" --include 'src/**' --output-file "$tmp_root/incremental-options-seed.xml"
"$bin" "$incremental_options" --incremental --output-file "$tmp_root/incremental-options-changed.xml"
grep -Fq 'path="Cargo.toml"' "$tmp_root/incremental-options-changed.xml"
grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-options-changed.xml"

incremental_exclude="$tmp_root/incremental-exclude"
make_flag_repo "$incremental_exclude"
"$bin" "$incremental_exclude" --exclude '**/skip.rs' --output-file "$tmp_root/incremental-exclude-seed.xml"
"$bin" "$incremental_exclude" --incremental --output-file "$tmp_root/incremental-exclude-changed.xml"
grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-exclude-changed.xml"
grep -Fq 'path="src/skip.rs"' "$tmp_root/incremental-exclude-changed.xml"

incremental_base="$tmp_root/incremental-base"
incremental_current="$tmp_root/incremental-current"
make_golden_repo "$incremental_base"
make_golden_repo "$incremental_current"
"$bin" "$incremental_current" --incremental-base "$incremental_base" --output-file "$tmp_root/incremental-base-same.xml"
if grep -Fq 'path="Cargo.toml"' "$tmp_root/incremental-base-same.xml" || grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-base-same.xml"; then
  printf 'matching incremental base included files\n' >&2
  exit 1
fi

cat >> "$incremental_current/src/lib.rs" <<'RS'

pub fn changed() {}
RS
"$bin" "$incremental_current" --incremental-base "$incremental_base" --output-file "$tmp_root/incremental-base-changed.xml"
grep -Fq 'path="src/lib.rs"' "$tmp_root/incremental-base-changed.xml"
if grep -Fq 'path="Cargo.toml"' "$tmp_root/incremental-base-changed.xml"; then
  printf 'incremental base included unchanged manifest\n' >&2
  exit 1
fi

rm "$incremental_current/Cargo.toml"
"$bin" "$incremental_current" --incremental-base "$incremental_base" --incremental-summary --output-file "$tmp_root/incremental-summary.xml" > "$tmp_root/incremental-summary.txt"
grep -Fq 'incremental_summary:' "$tmp_root/incremental-summary.txt"
grep -Fq '  added: 0' "$tmp_root/incremental-summary.txt"
grep -Fq '  changed: 1' "$tmp_root/incremental-summary.txt"
grep -Fq '  unchanged: 0' "$tmp_root/incremental-summary.txt"
grep -Fq '  skipped: 0' "$tmp_root/incremental-summary.txt"
grep -Fq '  deleted: 1' "$tmp_root/incremental-summary.txt"
grep -Fq '<deleted path="Cargo.toml" />' "$tmp_root/incremental-summary.xml"

empty_repo="$tmp_root/empty-repo"
mkdir -p "$empty_repo"
if "$bin" "$empty_repo" --fail-on-empty --output-file "$tmp_root/empty.xml" >/dev/null 2>&1; then
  printf 'empty repo did not fail with --fail-on-empty\n' >&2
  exit 1
fi

unsupported_repo="$tmp_root/unsupported-repo"
mkdir -p "$unsupported_repo"
printf 'hello\n' > "$unsupported_repo/notes.txt"
if "$bin" "$unsupported_repo" --fail-on-empty --output-file "$tmp_root/unsupported.xml" >/dev/null 2>&1; then
  printf 'unsupported-only repo did not fail with --fail-on-empty\n' >&2
  exit 1
fi

if "$bin" "$golden_repo" --include '[' --output-file "$tmp_root/bad-glob.xml" >/dev/null 2>&1; then
  printf 'invalid glob did not fail\n' >&2
  exit 1
fi

unreadable_repo="$tmp_root/unreadable-repo"
mkdir -p "$unreadable_repo"
printf 'pub fn hidden() {}\n' > "$unreadable_repo/hidden.rs"
chmod 000 "$unreadable_repo/hidden.rs"
if "$bin" "$unreadable_repo" --output-file "$tmp_root/unreadable.xml" >/dev/null 2>&1; then
  printf 'unreadable file did not fail\n' >&2
  exit 1
fi
chmod 600 "$unreadable_repo/hidden.rs"

printf 'cli integration passed\n'
