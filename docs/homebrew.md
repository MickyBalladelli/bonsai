# Homebrew

The repository includes a source-build formula at
`Formula/bonsai.rb`. From a checkout, install it with:

```sh
brew install --build-from-source ./Formula/bonsai.rb
```

This builds the tagged source with Homebrew's Rust dependency. Verify the
installation:

```sh
bonsai --version
bonsai setup
```

For users to run `brew install bonsai`, publish the formula in a separate
`MickyBalladelli/homebrew-bonsai` tap repository. Then users can run:

```sh
brew tap MickyBalladelli/bonsai
brew install bonsai
```

When releasing a new Bonsai version, update the formula URL, SHA-256, and
version together. Compute the source archive checksum with:

```sh
shasum -a 256 bonsai-<version>.tar.gz
```
