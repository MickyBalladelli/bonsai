#!/usr/bin/env bash
# Bump the Bonsai version everywhere it is recorded.
#
# Cargo.toml is the source of truth; scripts/sync-version.rb propagates the
# version to plugin manifests, the VS Code package and lockfile, Cargo.lock,
# and the Homebrew tag, and docs/cli-options.md is regenerated.
#
# Usage:
#   ./bump-version.sh 0.5.31 [--commit]
#
# With --commit, the version files are committed as "Bump version to X".
# The working tree must be clean before running.
#
# After bumping: push, tag v<version> (release.yml builds on "v*" tags),
# then update Formula/bonsai.rb sha256 once the release archive exists.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

usage() {
  printf 'usage: ./bump-version.sh <version> [--commit]\n' >&2
  exit 1
}

version="${1:-}"
commit=""
if [[ "${2:-}" == "--commit" ]]; then
  commit=1
elif [[ $# -gt 1 ]]; then
  usage
fi

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  printf 'invalid version: %s (expected semver like 0.5.31)\n' "$version" >&2
  exit 1
fi

if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  printf 'working tree has tracked modifications; commit or stash first\n' >&2
  exit 1
fi

NEW_VERSION="$version" ruby -i -pe 'if !$bumped && $_.match?(/\Aversion = "[^"]+"/); $_ = %Q{version = "#{ENV["NEW_VERSION"]}"\n}; $bumped = true; end' Cargo.toml
grep -n '^version = ' Cargo.toml | head -n 1

ruby scripts/sync-version.rb
sh scripts/generate-cli-reference.sh
ruby scripts/sync-version.rb --check

if [[ -n "$commit" ]]; then
  version_files=(
    Cargo.toml
    Cargo.lock
    docs/cli-options.md
    .claude-plugin/marketplace.json
    claude/bonsai/.claude-plugin/plugin.json
    plugins/bonsai/.codex-plugin/plugin.json
    copilot/bonsai-vscode/package.json
    copilot/bonsai-vscode/package-lock.json
    Formula/bonsai.rb
  )
  staged=()
  for file in "${version_files[@]}"; do
    if [[ -n "$(git status --porcelain -- "$file")" ]]; then
      staged+=("$file")
    fi
  done
  if [[ "${#staged[@]}" -eq 0 ]]; then
    printf 'nothing to commit for version %s\n' "$version"
    exit 0
  fi
  git add -- "${staged[@]}"
  git commit -m "Bump version to $version"
fi

printf 'Version is now %s.\n' "$version"
printf 'Next: push, tag v%s to trigger the release, then update the Homebrew sha256.\n' "$version"
