#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -n "${BONSAI_BIN:-}" ]; then
  binary=$BONSAI_BIN
else
  binary=$repo_root/target/debug/bonsai
fi

if [ ! -x "$binary" ]; then
  cargo build --manifest-path "$repo_root/Cargo.toml"
fi

"$binary" docs > "$repo_root/docs/cli-options.md"
echo "wrote $repo_root/docs/cli-options.md"
