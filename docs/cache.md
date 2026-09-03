# Bonsai cache

Bonsai keeps parse results outside the repository. The location is:

```text
${temporary-directory}/bonsai-parse-cache/<repository-hash>.cache
```

The temporary directory comes from the operating system. On macOS it is usually
under `/var/folders/.../T`; on Linux it is usually `/tmp`. Get the exact path,
size, entry count, and stale-entry count with:

```sh
bonsai doctor
```

## Local incremental context

Build or refresh the baseline with one normal run, then select added and changed
files from that cache:

```sh
bonsai .
bonsai . --incremental --incremental-summary
```

The cache stores file size and modification time for each parsed file. Bonsai
re-parses files when either changes. It also stores file-selection settings. A
change to `--include`, `--exclude`, `--exclude-generated`,
`--max-file-bytes`, or Git-ignore handling causes the next incremental run to
refresh the selected baseline instead of trusting the old selection.

Clear one repository's cache when you want a clean baseline or `doctor` reports
stale entries:

```sh
bonsai cache clear
```

## Compare with another base

Use a repository directory or a saved Bonsai cache file as the baseline:

```sh
bonsai . --incremental-base ../old-checkout
bonsai . --incremental-base /path/to/base.cache
```

This does not use the current repository's local cache as the comparison base.

## Compare with Git

Use a Git ref when the question is about a branch or commit:

```sh
bonsai . --changed-since main --incremental-summary
```

This includes tracked changes, untracked supported files, and deletions. Do not
combine `--changed-since` with `--incremental` or `--incremental-base`.
