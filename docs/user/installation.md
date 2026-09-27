# Installation

Two routes: **Homebrew**, which handles mpv for you, or **building from source**. Standalone
binaries, the AUR, Scoop and WinGet are the next milestone — see [ROADMAP.md](../../ROADMAP.md).

## Homebrew (macOS and Linux)

```sh
brew tap tuturu742/tap
brew install loxia-player
```

mpv comes in as a dependency, so nothing else is needed. Note that Homebrew always installs its own
mpv into the Cellar even if you already have one from your distribution or MacPorts — it never
reuses system libraries, by design. You end up with a second copy, and loxia links against
Homebrew's.

Then skip to [Verifying the install](#verifying-the-install).

- [Homebrew](#homebrew-macos-and-linux)
- [Requirements](#requirements)
- [Installing mpv](#installing-mpv)
- [Installing the Rust toolchain](#installing-the-rust-toolchain)
- [Building](#building)
- [Verifying the install](#verifying-the-install)
- [Where loxia puts things](#where-loxia-puts-things)
- [Updating](#updating)
- [Uninstalling](#uninstalling)

## Requirements

| | Version | Why |
| :-- | :-- | :-- |
| **libmpv** | 0.35 or newer | The audio engine. loxia targets the libmpv 2.0 client API. |
| **Rust** | 1.97.1 | Pinned in `rust-toolchain.toml`; `rustup` honours it automatically inside the checkout. |
| **pkg-config** | any | Used at build time to locate libmpv when it is not on the linker's default path. Harmless if absent on a system where libmpv already is. |
| **A C linker** | any | Comes with `build-essential`, Xcode command-line tools, or MSVC. |
| **Emby server** | tested on 4.9.5 | Earlier 4.x servers are likely to work but are untested. |
| **Terminal** | 80×24 or larger | True colour recommended. Kitty or a Sixel-capable terminal for album art. |

Optional, and degraded quietly when missing:

- **libnotify** (Linux) — desktop notifications on track change.
- **A D-Bus session** (Linux) — MPRIS media-key control.

Linux is the primary development and test platform. macOS and Windows are supported by the code and
built in CI, but see the [platform notes](#platform-notes) below.

## Installing mpv

loxia links dynamically against libmpv. Both halves of the package are needed: the runtime library
to load at startup, and the development files (headers and the `.pc` file) to link against at build
time. On most distributions those are two packages.

| Platform | Command |
| :-- | :-- |
| Arch, Manjaro, CachyOS, EndeavourOS | `sudo pacman -S mpv pkgconf` |
| Debian, Ubuntu, Mint, Pop!_OS | `sudo apt install mpv libmpv-dev pkg-config build-essential` |
| Fedora, RHEL, Rocky | `sudo dnf install mpv mpv-libs-devel pkgconf-pkg-config gcc` |
| openSUSE | `sudo zypper install mpv mpv-devel pkg-config gcc` |
| Alpine | `sudo apk add mpv mpv-dev pkgconf build-base` |
| Void | `sudo xbps-install mpv mpv-devel pkg-config` |
| Gentoo | `sudo emerge media-video/mpv` |
| NixOS | add `mpv` to your environment, or `nix-shell -p mpv pkg-config` |
| macOS (Homebrew) | `brew install mpv pkg-config` |
| macOS (MacPorts) | `sudo port install mpv pkgconfig` |

### Windows

There is no single package to install. You need:

1. **The libmpv DLL** — from the `mpv-dev-x86_64-*.7z` archive of a
   [shinchiro release](https://github.com/shinchiro/mpv-winbuild-cmake/releases). Currently named
   `libmpv-2.dll`. Place it beside `loxia-player.exe`, or anywhere on `PATH`.
2. **An import library for linking.** The same archive carries `libmpv.dll.a`, which is GNU format
   and unusable by MSVC's linker — and no `mpv.def` and no `.lib`. For an MSVC build, derive the
   module definition from the DLL's own export table and build the import library from that:

   ```
   dumpbin -exports libmpv-2.dll
   :: take the name column into mpv.def as "LIBRARY libmpv-2.dll" + "EXPORTS" + one name per line
   lib -def:mpv.def -out:mpv.lib -machine:X64
   ```

   Then put the directory containing `mpv.lib` on the `LIB` environment variable before building.
   `.github/workflows/release.yml` does exactly this, if you want a working reference.

Alternatively, install mpv through [Scoop](https://scoop.sh) (`scoop install mpv`) or
[Chocolatey](https://chocolatey.org) (`choco install mpv`) for the runtime DLL, and fetch the `-dev`
archive separately for the link-time files.

### Verifying libmpv is visible

```sh
pkg-config --modversion mpv
```

This should print `2.x` (the client-API version, not mpv's own version number). If it prints
nothing, the development package is missing or `PKG_CONFIG_PATH` does not cover it.

## Installing the Rust toolchain

If you do not already have Rust:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

You do not need to pick a version. `rust-toolchain.toml` in the repository pins 1.97.1, and `rustup`
downloads and uses exactly that toolchain for any `cargo` command run inside the checkout.

## Building

```sh
git clone https://github.com/tuturu742/loxia-player.git
cd loxia-player
cargo build --release
```

The binary lands at `target/release/loxia-player`. Run it from there, or install it onto your `PATH`:

```sh
cargo install --path crates/loxia-player
```

`cargo install` places it in `~/.cargo/bin`, which `rustup` normally adds to `PATH` already.

A release build of the whole workspace takes a few minutes on a modern machine. For a quicker
iteration loop while trying things out, `cargo run -p loxia-player` builds in debug mode — noticeably
slower to start and to render, but fine for a smoke test.

### Build-time options

There are no Cargo features to choose. The workspace has a single binary crate (`loxia-player`) and
five libraries it links; everything is compiled in.

### Platform notes

**Linux.** The tested path. If libmpv lives somewhere non-standard — a Homebrew-on-Linux prefix, a
manual `/opt` install — the build script probes `pkg-config` and adds both the link path and a
matching rpath, so the binary finds the library at runtime without `LD_LIBRARY_PATH`. Make sure
`pkg-config --modversion mpv` works before building.

**macOS.** Builds against the Homebrew or MacPorts mpv. Both Apple Silicon and Intel targets work.
Since there is no signed installer yet, you are building the binary yourself and Gatekeeper is not
involved.

**Windows.** Builds in CI, but has had less real-world exercise than Linux. The link-time
prerequisites above are the usual sticking point. Windows Terminal is recommended; the classic
`conhost` console lacks the colour and mouse support the UI expects.

## Verifying the install

```sh
loxia-player --doctor
```

This prints a diagnostics report and exits. What to look for:

```text
Audio
  ok    libmpv        v0.41.0 — /usr/lib/libmpv.so.2.5.0
  ...
  ok    devices       12 found
```

If libmpv were missing, that line would read `FAIL  libmpv  not found` and the summary at the bottom
would report a failure. `--doctor` exits non-zero when anything fails, so it works as a scripted
pre-flight check too.

The `Terminal` section confirms which album-art protocol was detected and the terminal's size; the
`Storage` section shows the paths loxia resolved on your system.

To browse a library without libmpv at all — useful for checking the rest of the install on a
headless machine — pass `--no-audio`. It substitutes a silent mock engine: the queue advances and
the position counter runs, but nothing is played.

## Where loxia puts things

loxia follows each platform's own conventions and creates nothing until it needs to.

| | Linux / BSD | macOS | Windows |
| :-- | :-- | :-- | :-- |
| **Config** | `~/.config/loxia-player/` | `~/Library/Application Support/loxia-player/` | `%APPDATA%\loxia-player\` |
| **Track cache** | `~/.cache/loxia-player/` | `~/Library/Caches/loxia-player/` | `%LOCALAPPDATA%\loxia-player\cache\` |
| **Downloads** | `~/.local/share/loxia-player/` | `~/Library/Application Support/loxia-player/` | `%APPDATA%\loxia-player\` |
| **State and logs** | `~/.local/state/loxia-player/` | *(same as Downloads)* | *(same as Downloads)* |

macOS and Windows have no separate "state" directory, so the session snapshot, listening history,
scrobble buffer and log files sit alongside the downloads there.

Both the cache and the download roots can be relocated with `cache.cache_dir` and
`cache.download_dir` — see [Configuration](configuration.md#cache). `loxia-player --doctor` always
prints the paths actually in use.

## Updating

```sh
cd loxia-player
git pull
cargo build --release
```

Or, if you installed with `cargo install`, `cargo install --path crates/loxia-player --force`.

Your config, cache, downloads and session are all outside the checkout and survive a rebuild
untouched. If a release ever changes the config format incompatibly, `schema_version` is bumped and
the file is migrated in place on first launch.

## Uninstalling

Remove the binary — delete `target/release/loxia-player`, or run
`cargo uninstall loxia-player` if you installed it that way.

To remove everything else, delete the four directories from the table above. On Linux:

```sh
rm -rf ~/.config/loxia-player ~/.cache/loxia-player \
       ~/.local/share/loxia-player ~/.local/state/loxia-player
```

The download directory is the one worth checking first — it holds the music you pinned for offline
listening, which may be several gigabytes.

## Troubleshooting

If the build or the first launch does not go as described here, see
[Troubleshooting](troubleshooting.md). The most common problems are a missing libmpv development
package and a terminal too small for the layout.
