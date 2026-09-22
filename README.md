<p align="center">
  <img src="assets/logo.svg" width="160" alt="loxia logo: a crossbill above a terminal cursor">
</p>

# loxia

A keyboard-driven terminal music client for Emby servers.

## What it is

loxia plays music from an Emby server in a terminal user interface. It is designed for keyboard use and can continue to browse and play downloaded music through its local cache when the server is unavailable.

The workspace separates its domain state, reducer, queue, configuration, and effects from network, cache, audio, and terminal I/O. That keeps the central application logic pure and independently testable.

loxia is licensed under [GPL-3.0-or-later](LICENSE). It is part of a Fringillidae-themed family of applications; its sibling is `pyrrhula`, named for the bullfinches, while *Loxia* is the crossbill genus.

## Features

- Browse Emby music libraries, folders, genres, favourites, playlists, artists, albums, and tracks.
- Search the Emby library and build and manage a playback queue.
- Play audio through mpv/libmpv, including next-track preloading for gapless playback.
- Use a 10-band equalizer with shipped factory presets.
- Apply ReplayGain in album, track, or off modes.
- Enumerate and select audio output devices.
- Download music for offline use, with a cache manifest, LRU handling, and an offline browse index.
- Persist pending scrobbles and playback session/history data.
- Choose from the shipped themes: `amber_crt`, `cyberpunk_neon`, `darcula`, `default_terminal`, `far_blue`, `green_crt`, and `oled_black`.
- Control the interface with the configurable keyboard keymap.

## Status

loxia is an unreleased source-build project under active development. The current scope and milestones are tracked in the [roadmap](docs/ROADMAP.md).

## Requirements

- Rust **1.97.1** or newer, with the Rust **2024 edition** toolchain.
- A native libmpv installation, including mpv development headers and `pkg-config`.
- An Emby server with a music library and credentials for an Emby user.

Install the native build dependencies before building:

```sh
# Debian / Ubuntu
sudo apt update
sudo apt install build-essential pkg-config libmpv-dev
```

```sh
# Fedora
sudo dnf install gcc pkgconf-pkg-config mpv-devel
```

```sh
# Arch Linux
sudo pacman -S --needed base-devel pkgconf mpv
```

```sh
# macOS / Homebrew
brew install mpv pkg-config
```

Install Rust with [rustup](https://rustup.rs/) if it is not already available:

```sh
rustup toolchain install 1.97.1
```

## Install

Clone the repository and build the workspace in release mode:

```sh
git clone https://github.com/tuturu742/loxia-player.git
cd loxia-player
cargo build --release
```

The player executable is written to:

```text
target/release/loxia-player
```

It can also be installed from the workspace checkout:

```sh
cargo install --path crates/loxia-player
```

There are currently no prebuilt packages or released binaries; see the [roadmap](docs/ROADMAP.md).

## Usage

Launch the release build from the repository root:

```sh
./target/release/loxia-player
```

Or, after `cargo install`:

```sh
loxia-player
```

On first run, create the configuration file at the platform configuration location:

| Platform | Configuration file |
|---|---|
| Linux | `$XDG_CONFIG_HOME/loxia/config.toml`, or `~/.config/loxia/config.toml` when `XDG_CONFIG_HOME` is unset |
| macOS | `~/Library/Application Support/loxia/config.toml` |
| Windows | `%APPDATA%\loxia\config.toml` |

Configure an Emby server URL and the credentials used to authenticate with it. A minimal configuration is:

```toml
[server]
url = "https://emby.example.example"
username = "your-emby-user"
password = "your-emby-password"

[ui]
theme = "default_terminal"
```

Set `ui.theme` to one of the shipped theme names, for example:

```toml
[ui]
theme = "cyberpunk_neon"
```

The default bindings and keymap format are documented in [the state and input reference](docs/04-state-and-input.md).

## Architecture

| Crate | Responsibility |
|---|---|
| `loxia-core` | Pure domain state, reducers, effects, configuration, keymap, queue logic, and shared models. |
| `loxia-emby` | Async Emby REST and WebSocket client, authentication, queries, playback reporting, and stream URLs. |
| `loxia-audio` | libmpv-backed audio playback, gapless preloading, equalizer, ReplayGain, and audio-device support. |
| `loxia-cache` | Cache layout, downloads, manifests, LRU handling, offline index, scrobbles, and session persistence. |
| `loxia-tui` | ratatui terminal presentation, views, widgets, themes, and modal interfaces. |
| `loxia-player` | Application binary that starts the runtime and connects the UI, domain layer, workers, cache, network, and audio backend. |

`loxia-core` is pure domain logic and defines effects; `loxia-emby` and `loxia-cache` supply I/O; `loxia-audio` plays audio. The terminal UI and player application wire those layers together.

## Documentation

- [Documentation index](docs/README.md)
- [Roadmap](docs/ROADMAP.md)
- [Contributing](CONTRIBUTING.md)

## License

loxia is distributed under the [GNU General Public License v3.0 or later](LICENSE) (`GPL-3.0-or-later`).
