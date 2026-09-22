# Roadmap

This file is the single list of work that is planned but not done. Anything not listed here is either shipped or not planned. This is not a commitment and carries no dates or owners.

## Distribution & packaging

- **Automated tagged releases and prebuilt binaries**
  
  Add a release workflow that builds and publishes downloadable binaries from version tags. The repository currently has CI only; it has no tagged-release process or release artefacts.

- **Windows distribution**
  
  Produce a Windows installer or equivalent packaged application, including the required libmpv runtime and application metadata.

- **macOS distribution**
  
  Produce a macOS application or distributable archive with the required runtime dependencies and platform metadata.

- **Debian and Ubuntu packages**
  
  Provide installable Debian-family packages and repository or release-install instructions appropriate to them.

- **Fedora packages**
  
  Provide an RPM packaging path for Fedora and related distributions.

- **Arch Linux and AUR packaging**
  
  Provide an Arch package definition and, where appropriate, an AUR publication path.

- **Nix packaging**
  
  Add a Nix expression, flake, or other supported Nix installation path.

- **Homebrew formula**
  
  Provide a Homebrew formula for macOS and Linux users.

- **Container image**
  
  Publish a container image for environments that run terminal applications in containers.

- **Static and musl builds**
  
  Provide supported static or musl-targeted release builds where libmpv and the platform runtime can be packaged correctly.

- **crates.io publishing**
  
  Decide and implement publication of the workspace crates that are intended for external Rust consumers. No crate publishing workflow or registry release exists.

- **Shell completions and manual pages**
  
  Generate and install shell completions and man pages with packaged releases.

- **Code signing and release verification**
  
  Sign supported platform artefacts and publish checksums or other verification material with releases.

## Features

### loxia-core

No unshipped user-visible core feature is currently recorded here.

### loxia-emby

No unshipped user-visible Emby-client feature is currently recorded here.

### loxia-audio

No unshipped user-visible audio feature is currently recorded here.

### loxia-cache

No unshipped user-visible cache or offline feature is currently recorded here.

### loxia-tui

No unshipped user-visible TUI feature is currently recorded here.

### loxia-player

No unshipped user-visible player feature is currently recorded here.

## Known gaps

- **Bit-perfect playback**
  
  The removed bit-perfect-output design remains a limitation intended for later reconsideration. See the bit-perfect-mode decision row in [docs/12-decisions.md](12-decisions.md).

- **Live equalizer filter mutation**
  
  Equalizer changes currently use the documented whole-filter replacement workaround rather than mpv's intended live `af-command` route. See the equalizer filter-update decision row in [docs/12-decisions.md](12-decisions.md).
