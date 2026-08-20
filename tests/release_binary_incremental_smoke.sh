#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'usage: %s /path/to/bonsai-binary\n' "$0" >&2
  exit 2
fi

bin="$1"
tmp_repo="$(mktemp -d "${TMPDIR:-/tmp}/bonsai-release-binary.XXXXXX")"
trap 'rm -rf "$tmp_repo"' EXIT

mkdir -p "$tmp_repo/src"
cat > "$tmp_repo/src/lib.rs" <<'RS'
pub fn greet() -> &'static str {
    "hello"
}
RS

"$bin" "$tmp_repo" --incremental --output-file "$tmp_repo/first.xml"
grep -Fq 'path="src/lib.rs"' "$tmp_repo/first.xml"

"$bin" "$tmp_repo" --incremental --output-file "$tmp_repo/second.xml"
if grep -Fq 'path="src/lib.rs"' "$tmp_repo/second.xml"; then
  printf 'release binary incremental run included unchanged file\n' >&2
  exit 1
fi
