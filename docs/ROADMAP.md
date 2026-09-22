# Roadmap

This file is the single list of work that is planned but not done; anything not listed here is either shipped or not planned. It is not a commitment and carries no dates or owners.

## Distribution & packaging

- **Tagged release process and prebuilt binaries.** Publish reproducible release artifacts for supported Windows, macOS, and Linux targets through a tagged release process. The current CI workflow validates the workspace but does not build or upload release binaries.

- **Static and musl Linux builds.** Provide a static or musl-oriented Linux distribution where the audio dependency constraints can be documented and supported. No such build target or release artifact exists.

- **Windows installer.** Package the Windows application and its required runtime dependencies as an installable distribution. There is no Windows packaging configuration or installer artifact.

- **macOS application distribution.** Package loxia for macOS as a distributable application with its runtime dependencies. There is no macOS bundle or distribution process.

- **Debian and Ubuntu packages.** Provide maintained Debian-family packages and repository metadata. No Debian control files, package build rules, or published packages exist.

- **Fedora packages.** Provide Fedora packaging and a supported installation channel. No RPM specification or Fedora distribution configuration exists.

- **Arch and AUR packages.** Provide an Arch package definition or AUR distribution path. No PKGBUILD or AUR package exists.

- **Nix packaging.** Add a Nix expression, flake, or other supported Nix installation path. The repository contains no Nix packaging files.

- **Homebrew formula.** Provide a Homebrew formula or tap for macOS and Linux users. No formula or tap is maintained.

- **Container image.** Publish a container image for environments where running the client in a container is appropriate. No container build definition or image publication process exists.

- **crates.io publishing.** Decide whether the workspace crates should be published and, if so, configure and publish them as supported crates. The workspace has no crates.io publishing process.

- **Shell completions and manual pages.** Generate and install shell-completion files and man pages with packaged distributions. Neither install rules nor generated documentation artifacts exist.

- **Release signing.** Add code signing and verification for distributed binaries and installers. No signing configuration, keys, or verification metadata is part of the release process.

## Features

### loxia-core

There is no separately tracked unshipped `loxia-core` feature work.

### loxia-emby

There is no separately tracked unshipped `loxia-emby` feature work.

### loxia-audio

- **Bit-perfect output mode.** Restore a supported bit-perfect playback mode, including capability handling and user-facing configuration. The earlier platform-specific approach was removed rather than shipped.

### loxia-cache

There is no separately tracked unshipped `loxia-cache` feature work.

### loxia-tui

There is no separately tracked unshipped `loxia-tui` feature work.

### loxia-player

There is no separately tracked unshipped `loxia-player` feature work.

## Known gaps

- **Bit-perfect playback is unavailable.** The deferred implementation and the reason the prior approach was removed are recorded in the [bit-perfect decision row](12-decisions.md#9-deviations-from-specification). This remains the user-visible limitation tracked by the `loxia-audio` roadmap entry above.
