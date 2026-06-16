#!/usr/bin/env bash
set -euo pipefail

repo_path="${1:-.}"
max_tokens="${2:-12000}"
level="${3:-2}"
output_file="${4:-/tmp/contextshrink.xml}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd "$script_dir/../../../../.." && pwd)"
default_bin="$project_root/target/release/contextshrink"

if [[ -n "${CONTEXTSHRINK_BIN:-}" ]]; then
  bin="$CONTEXTSHRINK_BIN"
elif command -v contextshrink >/dev/null 2>&1; then
  bin="contextshrink"
else
  bin="$default_bin"
fi

if [[ ! -x "$bin" && "$bin" == "$default_bin" ]]; then
  cargo_path="${CARGO:-cargo}"
  if ! command -v "$cargo_path" >/dev/null 2>&1 && [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    cargo_path="$HOME/.cargo/bin/cargo"
  fi
  "$cargo_path" build --release --manifest-path "$project_root/Cargo.toml"
fi

"$bin" "$repo_path" \
  --max-tokens "$max_tokens" \
  --level "$level" \
  --output file \
  --output-file "$output_file"

printf '%s\n' "$output_file"
