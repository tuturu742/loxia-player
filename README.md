# loxia

<p align="center">
  <img src="assets/logo.svg" width="160" alt="loxia logo: a crossbill above a terminal cursor">
</p>

A keyboard-driven terminal music client for Emby servers.

## What it is

loxia plays music from an Emby server in a terminal user interface. It is designed for keyboard use, with library browsing, queue management, and playback available without leaving the terminal.

Downloaded music and its library metadata are kept in a local cache, allowing cached content to remain available when the server cannot be reached. The workspace keeps domain state, reducers, queue handling, configuration, and effects separate from network, cache, audio, and terminal I/O, so the central application logic is pure and testable.

loxia is licensed under [GPL-3.0-or-later](LICENSE). It belongs to a Fringillidae-themed family of applications alongside its sibling, `pyrrhula`; *Loxia* is the crossbill genus.

## Features

- Browse Emby music libraries, folders, genres, favourites, playlists, artists, albums, and tracks.
- Search the library and build, reorder, shuffle, and sort a playback queue.
- Play audio through mpv/libmpv, including next-track preloading for gapless playback.
- Use a 10-band equalizer with factory presets.
- Apply ReplayGain in album, track, or off modes.
- Enumerate and select audio output devices.
- Cache and download music for offline use, with a cache manifest, LRU handling, and an offline browse index.
- Persist pending scrobbles and playback session and history data.
- Choose from the shipped themes: `amber_crt`, `cyberpunk_neon`, `darcula`, `default_terminal`, `far_blue`, `green_crt`, and `oled_black`.
- Control the interface through a configurable keyboard keymap.

## Status

loxia is an unreleased source-build project under active development. Its current scope and milestones are tracked in the [roadmap](docs/ROADMAP.md).

## Requirements

- Rust **1.97.1**, using the Rust **2024 edition**.
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

Install the required Rust toolchain with [rustup](https://rustup.rs/) if it is not already available:

```sh
rustup toolchain install 1.97.1
```

## Install

Clone the repository and build the workspace in release mode:

```sh
git clone https://github.com/tuturu742/loxia.git
cd loxia
cargo build --release
```

The release binaries are written to:

```text
target/release/loxia-tui
target/release/loxia-player
```

Each binary can also be installed from the workspace checkout:

```sh
cargo install --path crates/loxia-tui
cargo install --path crates/loxia-player
```

There are currently no prebuilt packages or released binaries; see the [roadmap](docs/ROADMAP.md).

## Usage

Launch either release binary from the repository root:

```sh
./target/release/loxia-tui
```

```sh
./target/release/loxia-player
```

After installing them with Cargo, the corresponding commands are available on `PATH`:

```sh
loxia-tui
```

```sh
loxia-player
```

On first run, create the configuration file at the platform configuration location:

| Platform | Configuration file |
|---|---|
| Linux | `$XDG_CONFIG_HOME/loxia/config.toml`, or `~/.config/loxia/config.toml` when `XDG_CONFIG_HOME` is unset |
| macOS | `~/Library/Application Support/loxia/config.toml` |
| Windows | `%APPDATA%\loxia\config.toml` |

Configure the Emby server URL and the Emby username and password used to authenticate with it. A minimal configuration is:

```toml
[server]
url = "https://emby.example"
username = "your-emby-user"
password = "your-emby-password"

[ui]
theme = "default_terminal"
```

Select a shipped theme by setting `ui.theme`, for example:

```toml
[ui]
theme = "cyberpunk_neon"
```

See [the state and input reference](docs/04-state-and-input.md) for the default keybindings and keymap format.

## Architecture

| Crate | Responsibility |
|---|---|
| `loxia-core` | Pure domain state, reducers, queue logic, configuration, keymaps, and effect definitions. |
| `loxia-emby` | Emby authentication, API client, endpoint access, streaming URLs, and WebSocket communication. |
| `loxia-cache` | Local cache layout, manifests, downloads, offline index, scrobble buffer, and session persistence. |
| `loxia-audio` | Audio backend abstraction and libmpv playback, including devices, EQ, gapless playback, and ReplayGain. |
| `loxia-tui` | Ratatui terminal presentation, views, widgets, themes, modal interfaces, and the `loxia-tui` binary. |
| `loxia-player` | Application runtime, terminal integration, worker orchestration, and the `loxia-player` binary. |

`loxia-core` is pure domain logic and defines effects; `loxia-emby` and `loxia-cache` supply I/O; `loxia-audio` plays audio. The `loxia-tui` and `loxia-player` binaries wire those layers together.

## Documentation

- [Documentation index](docs/README.md)
- [Roadmap](docs/ROADMAP.md)
- [Contributing](CONTRIBUTING.md)

## License

loxia is licensed under [GPL-3.0-or-later](LICENSE).
