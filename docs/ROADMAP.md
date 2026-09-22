# Roadmap

This file is the single list of work that is planned but not done. Anything not listed here is either shipped or not planned; this roadmap is not a commitment and carries no dates or owners.

## Distribution & packaging

- **Automated tagged releases and prebuilt binaries.** Establish a release process that produces versioned, downloadable binaries rather than requiring users to build the workspace themselves.

- **crates.io publishing.** Decide and implement publishing for any workspace crates intended to be consumed independently, including package metadata and release automation.

- **Windows distribution.** Produce a supported Windows package or installer that includes the executable, required runtime assets, and libmpv.

- **macOS distribution.** Produce a supported macOS package with the executable, required runtime assets, and documented libmpv handling.

- **Debian and Ubuntu packages.** Provide installable Debian-family packages and repository or release-artifact instructions.

- **Fedora packages.** Provide an installable Fedora package and the metadata needed to maintain it.

- **Arch Linux and AUR packaging.** Provide an Arch package definition or AUR package for installing loxia and its runtime dependencies.

- **Nix packaging.** Add a Nix expression, flake, or other maintained Nix installation channel.

- **Homebrew distribution.** Provide a Homebrew formula or tap for macOS and Linux Homebrew users.

- **Container image.** Publish a container image for environments where running the client in a container is appropriate.

- **Static and musl builds.** Evaluate and provide supported static or musl release artefacts where libmpv and the platform permit them.

- **Shell completions and manual pages.** Generate and install shell completion files and a man page as part of supported package builds.

- **Code signing and notarisation.** Sign release artefacts where platform conventions require it, including any applicable macOS notarisation and Windows signing work.

## Features

### `loxia-core`

No unshipped user-visible core feature is currently recorded.

### `loxia-emby`

No unshipped user-visible Emby-client feature is currently recorded.

### `loxia-audio`

No unshipped user-visible audio-engine feature is currently recorded.

### `loxia-cache`

No unshipped user-visible cache or offline feature is currently recorded.

### `loxia-tui`

No unshipped user-visible TUI feature is currently recorded.

### `loxia-player`

No unshipped user-visible player feature is currently recorded.

## Known gaps

- **Deferred implementation limitations.** Limitations explicitly intended for later follow-up remain documented in the deferred-work rows of [the decision log](12-decisions.md); that log is the authoritative context for those gaps rather than a duplicate specification here.
