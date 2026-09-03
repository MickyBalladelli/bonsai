#!/usr/bin/env ruby

root = File.expand_path("..", __dir__)
check_only = ARGV.delete("--check")
abort "usage: ruby scripts/sync-version.rb [--check]" unless ARGV.empty?

version_match = File.read(File.join(root, "Cargo.toml")).match(/^version\s*=\s*"([^"]+)"/)
abort "could not find package version in Cargo.toml" unless version_match

version = version_match[1]
unless version.match?(/\A\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?\z/)
  abort "invalid package version in Cargo.toml: #{version}"
end

problems = []

def sync_text(path, pattern, version, check_only, label, problems)
  text = File.read(path)
  matches = text.scan(pattern)
  abort "could not find #{label} in #{path}" if matches.empty?

  current_versions = matches.map { |match| match[1] }.uniq
  if check_only
    unless current_versions == [version]
      problems << "#{label} in #{path} is #{current_versions.join(", ")}, expected #{version}"
    end
    return
  end

  updated = text.gsub(pattern) do
    match = Regexp.last_match
    "#{match[1]}#{version}#{match[3]}"
  end
  File.write(path, updated) unless updated == text
end

json_version = /^([ \t]*"version": ")([^"]+)(")/
[
  ".claude-plugin/marketplace.json",
  "claude/bonsai/.claude-plugin/plugin.json",
  "plugins/bonsai/.codex-plugin/plugin.json",
  "copilot/bonsai-vscode/package.json"
].each do |relative_path|
  sync_text(
    File.join(root, relative_path),
    json_version,
    version,
    check_only,
    "version",
    problems
  )
end

package_lock = File.join(root, "copilot/bonsai-vscode/package-lock.json")
sync_text(package_lock, /^(  "version": ")([^"]+)(")/, version, check_only, "lockfile version", problems)
sync_text(
  package_lock,
  /^(    "": \{\n      "name": "bonsai-vscode",\n      "version": ")([^"]+)(")/,
  version,
  check_only,
  "extension lockfile package version",
  problems
)

cargo_lock = File.join(root, "Cargo.lock")
sync_text(
  cargo_lock,
  /(\[\[package\]\]\nname = "bonsai"\nversion = ")([^"]+)(")/,
  version,
  check_only,
  "Cargo.lock package version",
  problems
)

formula = File.join(root, "Formula/bonsai.rb")
formula_before = File.read(formula)
sync_text(
  formula,
  /(archive\/refs\/tags\/v)([^"]+)(\.tar\.gz)/,
  version,
  check_only,
  "Homebrew source tag",
  problems
)
formula_changed = File.read(formula) != formula_before unless check_only

if check_only
  if problems.empty?
    puts "version files are synchronized at #{version}"
  else
    warn problems.join("\n")
    exit 1
  end
else
  puts "synchronized version files at #{version}"
  if formula_changed
    puts "update Formula/bonsai.rb sha256 after the v#{version} release archive exists"
  end
end
