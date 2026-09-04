# Bonsai maintenance

## Keep the CLI reference current

The Rust `clap` command schema is the source of truth for CLI help and the
option reference. Generate the Markdown page after changing flags or commands:

```sh
sh scripts/generate-cli-reference.sh
```

The same schema drives `bonsai --help`, shell completions, and `bonsai docs`.
The VS Code extension has its own self-contained TypeScript generator and
intentionally exposes only its core settings.

## Keep versions current

`Cargo.toml` is the version source of truth. After changing it, synchronize the
VS Code package, plugin manifests, lockfiles, Homebrew tag, and release-facing
files:

```sh
ruby scripts/sync-version.rb
```

Check without changing files:

```sh
ruby scripts/sync-version.rb --check
```

The Homebrew source archive gets a new SHA-256 only after its matching Git tag
exists. Update that checksum in `Formula/bonsai.rb` before publishing the tap.

## Check health

Use the CLI health report before investigating parser or tokenizer problems:

```sh
bonsai --version
bonsai doctor
bonsai doctor --json
```

`bonsai setup` is the friendly first-run check. It shows the Bonsai version,
binary, tokenizer, parsers, clipboard warning, repository, and output path.

## Release checklist

1. Change the version in `Cargo.toml`.
2. Run `ruby scripts/sync-version.rb`.
3. Run `sh scripts/generate-cli-reference.sh`.
4. Run `cargo check` and `cd copilot/bonsai-vscode && npm run compile`.
5. Update `Formula/bonsai.rb` with the new source SHA-256.
6. Build release assets and verify their checksums.
