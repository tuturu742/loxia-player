# Roadmap

This file is the single list of work that is planned but not done; anything not listed here is either shipped or not planned. It is not a commitment and carries no dates or owners.

## Distribution & packaging

- **Tagged releases and prebuilt binaries.** Establish a release process that creates tagged releases and publishes downloadable binaries for supported platforms. The current CI workflow validates changes but does not produce release artefacts.

- **crates.io publishing.** Publish the workspace crates that are intended for external consumption, with a documented versioning and release process. No crate publishing configuration or publishing workflow exists.

- **Windows installer.** Package loxia-player for Windows, including the required libmpv runtime and application metadata. No Windows installer or other Windows package definition is present.

- **macOS application distribution.** Produce a macOS application package with the required runtime dependencies and application metadata. No macOS packaging or release artefact exists.

- **Debian and Ubuntu packages.** Provide installable Debian packages and an appropriate distribution channel for Debian-derived systems. No Debian control files, package build rules, or repository configuration exists.

- **Fedora packages.** Provide RPM packaging for Fedora. No spec file or Fedora build configuration exists.

- **Arch Linux and AUR packaging.** Provide an Arch package definition and AUR publication path. No PKGBUILD or AUR packaging metadata exists.

- **Nix packaging.** Add a Nix expression or flake that builds and installs loxia-player. No Nix files are present.

- **Homebrew formula.** Provide a Homebrew formula for macOS and Homebrew-on-Linux users. No formula or tap configuration exists.

- **Container image.** Publish a container image for supported non-interactive or remote-use cases. No container build definition or image publishing workflow exists.

- **Static and musl builds.** Produce static Linux release binaries where the audio dependency constraints permit them. No musl target build or static-release workflow exists.

- **Shell completions and manual pages.** Generate and install shell completions and man pages with packaged releases. No generated completion or man-page artefacts, nor installation rules for them, are present.

- **Release signing.** Sign release binaries and installers and publish verification material. No signing configuration or release-signing workflow exists.

## Features

### loxia-core

### loxia-emby

### loxia-audio

- **Bit-perfect output mode.** Restore a supported bit-perfect playback mode, including the capability checks and user-facing configuration needed to enable it safely. The earlier implementation was removed rather than exposing unreliable platform-specific behaviour.

### loxia-cache

### loxia-tui

### loxia-player

## Known gaps

- **Bit-perfect output limitation.** The current audio path does not provide bit-perfect output. See the deferred-work row for bit-perfect mode in [12-decisions.md](12-decisions.md); the implementation work is tracked above under `loxia-audio`.
