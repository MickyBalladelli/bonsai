#!/usr/bin/env bash
# Run the full Bonsai verification: every CI check plus the VSIX build.
#
# Mirrors .github/workflows/ci.yml (rust + vscode jobs) so a green local run
# means CI should pass too.
#
# Usage:
#   ./test-all.sh
#
# Requirements: rust toolchain (cargo), ruby, node 20+, python3.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

step() {
  printf '\n=== %s ===\n' "$1"
}

step "Check synchronized versions"
ruby scripts/sync-version.rb --check

step "Rust format check"
cargo fmt --check

step "Rust tests"
cargo test

step "Release build"
cargo build --release

step "Check generated CLI reference"
target/release/bonsai docs | diff -u docs/cli-options.md -

step "CLI integration tests"
BONSAI_BIN="$repo_root/target/release/bonsai" bash tests/cli_integration.sh

step "CLI output smoke test"
BONSAI_BIN="$repo_root/target/release/bonsai" bash tests/cli_output_smoke.sh

step "Plugin smoke tests"
bash tests/plugin_binary_lookup.sh

step "Release asset smoke test"
bash tests/release_assets_smoke.sh

step "Extension dependencies"
(cd copilot/bonsai-vscode && npm ci)

step "Extension tests"
(cd copilot/bonsai-vscode && npm test)

step "Package VSIX"
(cd copilot/bonsai-vscode && npm run package)
ls copilot/bonsai-vscode/bonsai-vscode-*.vsix

printf '\nAll tests passed and the VSIX is built.\n'
