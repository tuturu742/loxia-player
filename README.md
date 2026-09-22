# loxia

<p align="center">
  <img src="assets/logo.svg" width="160" alt="loxia logo: a crossbill and terminal cursor">
</p>

A keyboard-driven terminal music client for Emby, written in Rust.

## What it is

loxia lets you browse an Emby music library, build a queue, and play music without leaving the terminal. Its terminal interface is driven by the keyboard, while libmpv handles playback.

It can retain downloaded music and an offline library index locally, so cached content remains available when the server cannot be reached. Playback, session, and scrobble state are also persisted locally where applicable.

The workspace keeps its domain logic pure and testable: state transitions describe effects without performing I/O themselves. loxia is licensed under [GPL-3.0-or-later](LICENSE). It is part of a Fringillidae-themed family of projects; its sibling is `pyrrhula`.

## Features

- Browse Emby music libraries, including folders, genres, favourites, playlists, and search results.
- Build and manage a playback queue.
- Play audio through mpv/libmpv.
- Gapless next-track preloading.
- Ten-band equalizer with bundled factory presets.
- ReplayGain track and album modes.
- Audio-device enumeration and selection.
- Local cache, permanent downloads, and an offline browsing index.
- Persisted Emby scrobbles, listening session state, and history.
- Shipped themes: `amber_crt`, `cyberpunk_neon`, `darcula`, `default_terminal`, `far_blue`, `green_crt`, and `oled_black`.
- A fully keyboard-driven, configurable keymap.

## Status

loxia is a version `0.1.0` Rust workspace under active development. Its current scope, priorities, and remaining work are tracked in the [roadmap](docs/ROADMAP.md).

## Requirements

- Rust **1.97.1** or newer, with the Rust **2024** edition toolchain selected. The repository includes `rust-toolchain.toml`.
- Native **libmpv**, including its development headers and `pkg-config` metadata, for linking the `libmpv2` Rust binding.
- An Emby server and an Emby user account.

Install the native build dependencies for your platform:

```sh
# Debian / Ubuntu
sudo apt update
sudo apt install build-essential pkg-config libmpv-dev

# Fedora
sudo dnf install gcc pkgconf-pkg-config mpv-libs-devel

# Arch Linux
sudo pacman -S base-devel pkgconf mpv

# macOS / Homebrew
brew install pkg-config mpv
```

Install Rust with [rustup](https://rustup.rs/) if it is not already available:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Install

Build loxia from a checkout:

```sh
git clone https://github.com/tuturu742/loxia-player.git
cd loxia-player
cargo build --release
```

The executable is written to:

```text
target/release/loxia-player
```

You can also install the player crate into Cargo's binary directory:

```sh
cargo install --path crates/loxia-player
```

That installs `loxia-player` to `$CARGO_HOME/bin` (normally `~/.cargo/bin`).

This repository does not provide prebuilt packages or released binaries; see the [roadmap](docs/ROADMAP.md).

## Usage

Launch the player from a checkout:

```sh
target/release/loxia-player
```

Or, after `cargo install`:

```sh
loxia-player
```

On first run, configure an Emby server in the application settings. loxia reads its configuration from:

| Platform | Configuration file |
|---|---|
| Linux | `$XDG_CONFIG_HOME/loxia/config.toml` (usually `~/.config/loxia/config.toml`) |
| macOS | `~/Library/Application Support/loxia/config.toml` |
| Windows | `%APPDATA%\loxia\config.toml` |

A server profile needs the Emby server URL and the account credentials used to authenticate with it: `url`, `username`, and `password`. A minimal configuration is:

```toml
[[servers]]
name = "My Emby server"
url = "https://emby.example.example"
username = "my-emby-user"
password = "replace-with-your-password"

[interface]
theme = "default_terminal"
```

Replace the URL and credentials with those for your own server. Do not commit a configuration file containing real credentials.

Select a shipped theme by setting `interface.theme` to one of the theme names listed above, for example:

```toml
[interface]
theme = "cyberpunk_neon"
```

The in-application Settings view can also manage servers, audio settings, themes, and keybindings. For the keybinding reference, see [State and input](docs/04-state-and-input.md).

## Architecture

| Crate | Responsibility |
|---|---|
| `loxia-core` | Pure domain state, reducers, actions, effects, configuration, keymaps, queue logic, and shared models. |
| `loxia-emby` | Async Emby REST and WebSocket client, authentication, endpoint DTOs, retry logic, and stream URL construction. |
| `loxia-audio` | libmpv-backed playback, gapless preloading, equalizer, ReplayGain, and audio-device handling. |
| `loxia-cache` | Cache layout, manifests, downloads, LRU management, offline index, scrobble buffering, and session persistence. |
| `loxia-tui` | ratatui rendering, views, widgets, and terminal interaction presentation. |
| `loxia-player` | Application entry point, runtime, event dispatch, terminal setup, and I/O workers. |

`loxia-core` is pure domain logic and defines effects; `loxia-emby` and `loxia-cache` supply I/O; `loxia-audio` plays audio. The `loxia-tui` and `loxia-player` application crates wire those layers together.

## Documentation

- [Documentation index](docs/README.md)
- [Roadmap](docs/ROADMAP.md)
- [Contributing](CONTRIBUTING.md)

## License

loxia is licensed under the [GNU General Public License, version 3.0 or later](LICENSE).
