#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readme="$repo_root/README.md"
release="$repo_root/.github/workflows/release.yml"

required_assets=(
  "bonsai-linux-x64"
  "bonsai-macos-arm64"
  "bonsai-linux-x64.sha256"
  "bonsai-macos-arm64.sha256"
  "bonsai-vscode-*.vsix"
)

for asset in "${required_assets[@]}"; do
  if ! grep -Fq "$asset" "$readme"; then
    printf 'README missing release asset %s\n' "$asset" >&2
    exit 1
  fi
done

workflow_patterns=(
  'dist/bonsai-${{ matrix.name }}'
  'dist/bonsai-${{ matrix.name }}.sha256'
  'Smoke packaged binary'
  'bash tests/release_binary_incremental_smoke.sh "dist/bonsai-${{ matrix.name }}"'
  'Smoke downloaded Linux binary'
  'bash tests/release_binary_incremental_smoke.sh dist/bonsai-linux-x64'
  'copilot/bonsai-vscode/bonsai-vscode-*.vsix'
  'files: dist/*'
)

for pattern in "${workflow_patterns[@]}"; do
  if ! grep -Fq -- "$pattern" "$release"; then
    printf 'release workflow missing %s\n' "$pattern" >&2
    exit 1
  fi
done

printf 'release asset smoke passed\n'
