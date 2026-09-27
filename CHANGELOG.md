# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-rc.3] — 2026-09-27

### Security
- `rustls` 0.23.42 → 0.23.45 (RUSTSEC-2026-0285: TLS 1.3 handshake messages accepted across
  encryption level boundaries) and `h2` 0.4.15 → 0.4.19 (RUSTSEC-2026-0258: unbounded empty DATA
  frames). Both reached us transitively through `reqwest`. Neither had been caught because the
  `cargo deny check` step never actually ran — cargo-deny was not installed on the runner.

### Fixed
- CI now compiles. `ci.yml` predated `loxia-audio` and `souvlaki` and installed no system
  libraries, so every job on every platform had failed since the workflow was added — 0 successful
  runs out of 165. A shared `setup-libmpv` composite action now provides libmpv on all three
  platforms and libdbus on Linux.
- Four `paths.rs` tests asserted Unix path layouts unconditionally and failed on Windows. They had
  never run there, because the Windows job could not get as far as compiling.

## [0.1.0-rc.2] — 2026-09-27

### Fixed
- Widget snapshots no longer depend on the machine's timezone. The header clock and the
  listening-history rows render through the system zone, so five snapshots had encoded the
  UTC offset of whichever machine generated them and failed everywhere else — including CI,
  which runs UTC. `local_hour_minute` is now pinned to UTC under the dev-only `test-support`
  feature; a real build still shows local time.

### Added
- `.github/workflows/release.yml`: the release pipeline — Linux tarball built against an older
  glibc, Windows portable ZIP with libmpv bundled and hash-verified, Homebrew bottles, and the
  GitHub release itself.
- `COPYING.LGPL` and `packaging/windows/mpv.lock`, both required to redistribute libmpv on
  Windows.
- A Homebrew tap: `brew tap tuturu742/tap && brew install loxia-player`.

## [0.1.0-rc.1] — 2026-09-27

First release candidate. Everything below is the initial feature set; there is no prior release to
diff against.

**Installation is from source only.** Pre-built binaries, installers and package-manager entries are
the work remaining before 0.1.0 final — see [ROADMAP.md](ROADMAP.md).

### Browsing

- Miller-column navigation with a sliding three-column window and a `…/parent/` breadcrumb, across
  Artists, Album Artists, Albums, Genres and Folders.
- **ALBUMS / APPEARS ON** split on an artist's album list, separating their own releases from
  compilations and guest appearances, with distinct queueing semantics for each.
- Ten sidebar tabs — Now Playing, Favourites, Search, Playlists, Artists, Album Artists, Albums,
  Genres, Folders, Settings — reachable with `Alt+1`…`Alt+0`, with `F2`–`F10` aliases.
- Metadata inspector with artwork and a clickable action legend rendered from the live keymap.
- Fuzzy inline column filter (`/`) that keeps paging to find matches beyond the first screen.
- Library-wide search with results grouped by kind, favourites across artists/albums/tracks/playlists,
  and Emby playlists editable in place.
- Visual multi-select (`v`) for bulk queueing, playlist assignment and downloading.
- Incremental paging by keyboard and mouse wheel.

### Playback

- Gapless FLAC streaming via libmpv, with preloading of upcoming queue entries.
- Quality profiles cycled live: Direct, or server-side transcode at 320/192/96 kbps to MP3, AAC or Opus.
- 10-band ISO equalizer applied without restarting playback, with eight factory presets and
  user-defined curves.
- ReplayGain in album, track or off modes, with a configurable preamp.
- Output device enumeration and a picker grouped by driver. (Switching mid-track is unreliable — see
  Known limitations.)
- Sleep timer by duration, end of track or end of queue, with a fade-out.
- Non-destructive shuffle that restores the original order exactly, and multi-criteria sort profiles
  stacking up to four rules.
- Listening history with timestamps, and re-queueing from it.

### Offline

- Rolling LRU track cache with a configurable size limit, startup reconciliation and read-ahead of
  upcoming queue entries.
- Permanent pinned downloads of tracks, albums, artists and playlists, never evicted.
- Automatic offline detection after repeated failures, with a header badge, refusal toasts instead of
  hangs, and a probe-driven reconnect that also tries the profile's fallback addresses.
- Session persistence: queue, position, tab, volume, quality profile and EQ state, restored paused.

### Interface

- Seven themes: `default_terminal`, `far_blue`, `darcula`, `cyberpunk_neon`, `amber_crt`, `green_crt`,
  `oled_black`, plus an `ascii_only` glyph set.
- Album art in Kitty, Sixel and Unicode-halfblock protocols, auto-detected.
- Synced `.lrc` lyrics with a scrolling pane, and a Zen mode for artwork, title and lyrics alone.
- Mouse support: click to seek, scroll lists and columns, click tabs, rows and transport buttons.
- Context-aware help overlay rendered from the live keymap, so it can never drift from the real bindings.
- Toasts for every state change worth announcing, and empty states that name the key to press.

### System

- Multiple server profiles, each with any number of fallback addresses verified against the server's
  own identity before use, and arbitrary custom HTTP headers per address for reverse proxies and
  zero-trust gateways.
- Storage keyed on the Emby installation's identity rather than on the address, so profiles pointing at
  one server share a cache, a download tree and a session.
- MPRIS (Linux) and SMTC (Windows) media keys, and desktop notifications on track change.
- Emby WebSocket channel for remote control and live library updates.
- Full settings UI across nine sections, including an in-app keymapper with conflict detection.
- `loxia-player --doctor`: a redacted diagnostics report covering config, keybinding conflicts, libmpv,
  output devices, terminal graphics, storage and server reachability. Exits non-zero on any failure.

### Security and privacy

- The access token is stored `0600` on Unix, with a startup warning if the permissions have been
  widened, and is redacted from every log line, `Debug` impl and the About view's diagnostics.
- Custom header *values* are redacted the same way; names stay visible.
- Stream URLs carrying an `api_key` are redacted before they can reach a log.
- A CI check greps test fixtures for tokens and private IP addresses.

### Known limitations

- Offline *browsing* is limited to already-loaded columns; fetching a fresh column from the downloads
  index is not yet wired up.
- Playback reports made while offline are dropped rather than buffered and replayed on reconnect. The
  buffer itself (`loxia_cache::scrobble`) is implemented and tested, but nothing is connected to it.
- Switching the output device mid-track does not work reliably. Setting `audio.device_id` (or picking
  a device before playback starts) is the dependable route.
- Windows is functional and builds in CI, but has had much less real-world use than Linux.
- The access token is plaintext in `config.toml`; an OS-keyring backend is post-0.1.0.

[Unreleased]: https://github.com/tuturu742/loxia-player/compare/v0.1.0-rc.3...HEAD
[0.1.0-rc.3]: https://github.com/tuturu742/loxia-player/releases/tag/v0.1.0-rc.3
[0.1.0-rc.2]: https://github.com/tuturu742/loxia-player/releases/tag/v0.1.0-rc.2
[0.1.0-rc.1]: https://github.com/tuturu742/loxia-player/releases/tag/v0.1.0-rc.1
