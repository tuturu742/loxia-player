# Roadmap

This file is the single list of work that is planned but not done; anything not listed here is either shipped or not planned. It is not a commitment and carries no dates or owners.

## Distribution & packaging

- **Release automation and tagged GitHub releases**  
  Add a release workflow that builds, verifies, and publishes tagged releases instead of relying solely on the continuous-integration workflow.

- **Prebuilt release binaries**  
  Produce downloadable binaries for supported platforms and architectures, with release artefacts attached to published releases.

- **Windows distribution**  
  Provide a Windows installer or package that includes the required runtime dependencies and application assets.

- **macOS distribution**  
  Provide a macOS application package or installer with the required runtime dependencies and application assets.

- **Debian and Ubuntu packages**  
  Publish installable Debian-family packages with the binary, assets, desktop integration where applicable, and dependency metadata.

- **Fedora packages**  
  Provide a Fedora packaging route for installing loxia through the distribution’s package tooling.

- **Arch Linux and AUR packages**  
  Provide an Arch Linux package definition and, where appropriate, an AUR distribution path.

- **Nix packaging**  
  Add a Nix expression, flake, or other supported Nix installation route.

- **Homebrew distribution**  
  Provide a Homebrew formula or tap for macOS and supported Linux Homebrew installations.

- **Container image**  
  Publish a maintained container image for environments where running the client in a container is appropriate.

- **Static and musl builds**  
  Investigate and provide static or musl-targeted builds where libmpv and the other native dependencies can be supported correctly.

- **crates.io publishing**  
  Decide which workspace crates, if any, are suitable for publication and add the metadata and publishing process required for them.

- **Shell completions and man pages**  
  Generate and install command-line completions and manual pages through supported package and installation routes.

- **Release signing and checksums**  
  Sign release artefacts where supported and publish checksums or other integrity information alongside them.

## Features

### loxia-core

### loxia-emby

### loxia-audio

- **Bit-perfect output mode**  
  Restore a supported bit-perfect playback path, including the configuration and device-handling work needed to make its behaviour reliable across platforms.

### loxia-cache

### loxia-tui

### loxia-player

## Known gaps

- **Bit-perfect playback limitation**  
  The current playback path does not provide bit-perfect output. See the deferred-work row for `09-02` in [the decision log](12-decisions.md#9-decision-log).

- **In-place equalizer updates**  
  Equalizer changes currently use the compatible filter-graph path rather than mpv’s intended runtime filter-command route. See the equalizer row in [the decision log](12-decisions.md#9-decision-log).
