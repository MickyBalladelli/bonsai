#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/bonsai-plugin-smoke.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT

cargo_version="$(sed -nE 's/^version = "([^"]+)"/\1/p' "$repo_root/Cargo.toml" | head -n 1)"
plugin_version_files=(
  "$repo_root/plugins/bonsai/.codex-plugin/plugin.json"
  "$repo_root/claude/bonsai/.claude-plugin/plugin.json"
  "$repo_root/.claude-plugin/marketplace.json"
)

for version_file in "${plugin_version_files[@]}"; do
  if ! grep -Fq "\"version\": \"$cargo_version\"" "$version_file"; then
    printf '%s version does not match Cargo.toml %s\n' "$version_file" "$cargo_version" >&2
    exit 1
  fi
done

make_fake_bonsai() {
  local path="$1"
  local marker="$2"
  mkdir -p "$(dirname "$path")"
  cat > "$path" <<SCRIPT
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$marker" >> "\${BONSAI_MARKER_FILE:?}"
output_file=""
while [[ \$# -gt 0 ]]; do
  case "\$1" in
    --output-file)
      output_file="\${2:?}"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
if [[ -n "\$output_file" ]]; then
  mkdir -p "\$(dirname "\$output_file")"
  printf '<repository_context />\n' > "\$output_file"
fi
SCRIPT
  chmod +x "$path"
}

copy_helpers() {
  mkdir -p "$tmp_root/plugins/bonsai/skills/bonsai/scripts"
  mkdir -p "$tmp_root/claude/bonsai/bin"
  cp "$repo_root/plugins/bonsai/skills/bonsai/scripts/run_bonsai.sh" \
    "$tmp_root/plugins/bonsai/skills/bonsai/scripts/run_bonsai.sh"
  cp "$repo_root/claude/bonsai/bin/bonsai-claude" \
    "$tmp_root/claude/bonsai/bin/bonsai-claude"
}

run_case() {
  local helper="$1"
  local mode="$2"
  local expected="$3"
  local case_root="$tmp_root/$mode"
  local marker_file="$case_root/marker.txt"
  local output_file="$case_root/out.xml"
  local path_dir="$case_root/path-bin"

  mkdir -p "$case_root"
  rm -f "$marker_file"
  make_fake_bonsai "$case_root/env-bonsai" "BONSAI_BIN"
  make_fake_bonsai "$path_dir/bonsai" "PATH"
  make_fake_bonsai "$tmp_root/target/release/bonsai" "DEFAULT"

  case "$mode" in
    env)
      BONSAI_MARKER_FILE="$marker_file" \
        BONSAI_BIN="$case_root/env-bonsai" \
        PATH="$path_dir:/usr/bin:/bin" \
        "$helper" "$case_root" 12000 2 "$output_file" >/dev/null
      ;;
    path)
      BONSAI_MARKER_FILE="$marker_file" \
        BONSAI_BIN="" \
        PATH="$path_dir:/usr/bin:/bin" \
        "$helper" "$case_root" 12000 2 "$output_file" >/dev/null
      ;;
    default)
      BONSAI_MARKER_FILE="$marker_file" \
        BONSAI_BIN="" \
        PATH="/usr/bin:/bin" \
        CARGO="$case_root/missing-cargo" \
        "$helper" "$case_root" 12000 2 "$output_file" >/dev/null
      ;;
  esac

  actual="$(cat "$marker_file")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected %s, got %s for %s\n' "$expected" "$actual" "$helper" >&2
    exit 1
  fi
}

copy_helpers

for helper in \
  "$tmp_root/plugins/bonsai/skills/bonsai/scripts/run_bonsai.sh" \
  "$tmp_root/claude/bonsai/bin/bonsai-claude"
do
  run_case "$helper" env BONSAI_BIN
  run_case "$helper" path PATH
  run_case "$helper" default DEFAULT
done

printf 'plugin binary lookup smoke passed\n'
