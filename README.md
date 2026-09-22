# loxia

<p align="center">
  <img src="assets/logo.svg" width="160" alt="loxia crossbill logo">
</p>

A keyboard-driven terminal music client for Emby servers.

## What it is

loxia plays music from an Emby server in a terminal user interface. It is designed for keyboard-first library browsing, queue management, and playback, with mpv providing the audio engine.

The client can retain downloaded media and library information in a local cache, allowing cached music and an offline index to remain available when the server cannot be reached. Its domain logic is kept separate from I/O so state transitions, queue behavior, configuration, and key handling can be tested without a terminal, server, filesystem, or audio device.

loxia is licensed under [GPL-3.0-or-later](LICENSE). It belongs to a Fringillidae-themed family of applications; its sibling project is `pyrrhula`.

## Features

- Browse Emby music libraries, folders, artists, albums, genres, playlists, favourites, and search results.
- Build and control a playback queue through mpv/libmpv.
- Gapless next-track preloading through mpv's playlist support.
- Ten-band equalizer with factory presets.
- ReplayGain track and album modes.
- Audio-device enumeration and selection.
- Cached media downloads and an offline browsing index.
- Persisted scrobbles, listening sessions, and playback history.
- Shipped themes: `amber_crt`, `cyberpunk_neon`, `darcula`, `default_terminal`, `far_blue`, `green_crt`, and `oled_black`.
- A fully keyboard-driven, configurable keymap.

## Status

loxia is a pre-1.0 Rust workspace with working application components rather than a packaged release. There are currently no released binaries or distribution packages; project direction and remaining work are recorded in the [roadmap](docs/ROADMAP.md).

## Requirements

- A Rust toolchain supporting the Rust 2024 edition. The workspace MSRV is **Rust 1.97.1**.
- Native libmpv, including its development headers, for the `libmpv2` Rust bindings.
- An Emby server with a music library and a user account.

Install Rust with rustup, then install the required native packages for your platform:

```sh
rustup toolchain install 1.97.1
```

### Debian / Ubuntu

```sh
sudo apt update
sudo apt install build-essential pkg-config libmpv-dev
```

### Fedora

```sh
sudo dnf install gcc pkgconf-pkg-config mpv-libs-devel
```

### Arch Linux

```sh
sudo pacman -S --needed base-devel pkgconf mpv
```

### macOS / Homebrew

```sh
brew install pkg-config mpv
```

## Install

Build loxia from a checkout:

```sh
git clone https://github.com/tuturu742/loxia-player.git loxia
cd loxia
cargo build --release
```

The release binaries are written to:

```text
target/release/loxia-tui
target/release/loxia-player
```

They can also be installed into Cargo's binary directory, usually `~/.cargo/bin`:

```sh
cargo install --path crates/loxia-tui
cargo install --path crates/loxia-player
```

There are no prebuilt packages or released binaries at present; see the [roadmap](docs/ROADMAP.md).

## Usage

Run the terminal interface from a checkout:

```sh
target/release/loxia-tui
```

Run the player binary from a checkout:

```sh
target/release/loxia-player
```

After installation with `cargo install`, use the corresponding commands from your `PATH`:

```sh
loxia-tui
loxia-player
```

On first use, create the configuration file at:

| Platform | Configuration file |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/loxia/config.toml`, or `~/.config/loxia/config.toml` when `XDG_CONFIG_HOME` is unset |
| macOS | `~/Library/Application Support/loxia/config.toml` |
| Windows | `%APPDATA%\loxia\config.toml` |

The server configuration needs the Emby server URL, username, and password. A minimal configuration is:

```toml
[server]
url = "https://emby.example.com"
username = "your-emby-user"
password = "your-emby-password"

[ui]
theme = "default_terminal"
```

Set `ui.theme` to one of the shipped theme names to select it before launch. For example:

```toml
[ui]
theme = "cyberpunk_neon"
```

The application also exposes settings for changing the active theme and other preferences. See the [keybindings reference](docs/04-state-and-input.md) for keyboard controls rather than relying on a duplicated key list.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `loxia-core` | Pure domain logic: application state, reducers, effects, configuration, models, queue behavior, themes, and keymaps. |
| `loxia-emby` | Emby REST and WebSocket client, authentication, endpoint access, DTO conversion, retries, and stream URL construction. |
| `loxia-audio` | libmpv-backed audio playback, gapless preloading, equalizer support, ReplayGain, and audio-device handling. |
| `loxia-cache` | Local downloads, cache manifests and LRU handling, offline index data, scrobble buffering, and session persistence. |
| `loxia-tui` | ratatui terminal interface, layouts, views, widgets, modals, rendering, and theme styling. |
| `loxia-player` | Application bootstrap, terminal lifecycle, event dispatch, runtime wiring, and background workers. |

`loxia-core` is pure domain logic and defines effects; `loxia-emby` and `loxia-cache` supply I/O, while `loxia-audio` performs playback. The two binaries wire these layers together for terminal use.

## Documentation

- [Documentation index](docs/)
- [Roadmap](docs/ROADMAP.md)
- [Contributing](CONTRIBUTING.md)

## License

loxia is licensed under the [GNU General Public License, version 3.0 or later](LICENSE).
