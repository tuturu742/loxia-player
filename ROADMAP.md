# Roadmap

Where loxia is, and what is planned next. This is intent, not a schedule.

## 0.1.0 — release candidate

Feature-complete and in daily use. Browsing, playback, the queue engine, the cache and downloads,
the equalizer, ReplayGain, quality profiles, lyrics, artwork, Zen mode, mouse
support, media keys, notifications, WebSocket remote control, multi-server profiles with fallback
addresses, the settings UI and the in-app keymapper are all implemented.

**Distribution is done.** A tagged release builds and publishes a Linux tarball, a self-contained
Windows ZIP, and Homebrew bottles for Apple Silicon and Linux, and bumps the tap automatically.

Known gaps at rc:

Two subsystems are fully implemented and unit-tested in their own crate but never connected to the
running binary. Both are `loxia-cache` code reached through an `Effect` nothing produces or consumes:

- **Offline browsing.** `OfflineIndex` builds a browse tree from your pinned downloads, and
  `workers::network::try_serve_offline` knows how to answer a `FetchColumn` from it — but `bootstrap`
  hands the worker an `OfflineHandle` whose flag is never set and whose index is never populated, so
  that path is unreachable. Navigating somewhere new while disconnected reports a load failure instead
  of serving your downloads. Closing it means setting the flag when `reducer::connectivity` enters
  `Offline`, and building the index from `paths.downloads_root()`.
- **The offline scrobble buffer.** `loxia_cache::scrobble` implements the append/collapse/replay
  buffer, but nothing emits
  `CacheEffect::AppendScrobble` and the cache worker handles neither it nor `DrainScrobbles`. A
  playback report made while the server is unreachable is dropped with a log line, so offline
  listening never reaches your play counts.

Everything else about offline mode is live: detection after repeated failures, the header badge, the
refusal toasts, queue-entry availability recomputation, and the probe-driven reconnect that also tries
a profile's fallback addresses.

Other known gaps at rc:

- **Switching the output device mid-track is unreliable.** Enumeration and the picker itself work —
  `--doctor` and the `O` modal both list what mpv can see — but applying a change to a device while a
  track is playing does not dependably take effect. Setting `audio.device_id` in the config, or
  choosing a device before playback starts, is the route that works. Needs a proper diagnosis against
  real hardware before 0.1.0 final.
- **Windows has had far less real-world exercise than Linux.** It builds in CI and the code paths
  exist; treat it as beta.
- **No release artifacts.** See below.

## 0.1.0 — to ship

### Distribution

The whole reason this is still an rc.

| Target | Status | mpv |
| :-- | :-- | :-- |
| Linux `x86_64` | `.tar.gz` **shipping**, Homebrew bottle **shipping**; AUR outstanding | System libmpv from the distribution |
| Linux `aarch64` | deferred — needs a native runner or a cross sysroot | System libmpv |
| macOS Apple Silicon | Homebrew bottle **shipping**; `.dmg`/`.pkg` deferred | Homebrew's own mpv |
| macOS Intel | source build only — Homebrew ships no bottles for Tier 3 | Homebrew's own mpv, also from source |
| Windows `x86_64` | portable `.zip` **shipping**; `.msi`, Scoop, WinGet outstanding | `libmpv-2.dll` bundled beside the executable |

Driven by [`cargo-dist`](https://opensource.axo.dev/cargo-dist/) from a `v*` tag, with the pieces it
cannot do — WiX customisation for the mpv bundle, AUR publishing — as post-build steps in the same
workflow.

Specifics still to do:

- **`cargo-dist` configuration** and a release workflow triggered by a version tag.
- **Windows MSI** installing into `%ProgramFiles%\Loxia\` with `mpv-1.dll`, both licence texts and an
  uninstaller; a Start Menu entry that launches Windows Terminal; a portable ZIP that needs no install.
  The bundled DLL comes from a pinned upstream build with its SHA-256 recorded and verified in CI.
- **macOS `.dmg` and `.pkg`**, plus a Homebrew tap formula with `depends_on "mpv"`. Signed and notarised
  once a Developer ID is available; until then the `xattr -d com.apple.quarantine` workaround gets
  documented prominently rather than shipping an installer that fails Gatekeeper silently.
- **Linux tarball** built against an older glibc for reach, with a `loxia-player.1` man page, a
  `.desktop` file and icons; an AUR `PKGBUILD` with `depends=('mpv')` and `optdepends` for `libnotify`.
- **Licence-compliance checks in CI**, blocking any release that bundles mpv binaries without
  `COPYING.LGPL` present, with the library statically linked, or without the exact mpv version and
  source URL recorded in the release manifest. `release.yml`'s Windows job already enforces the
  first three.
- **Generated branding assets** — PNG icon sizes, `.ico`, `.icns` and the ASCII banner, from
  `assets/logo.svg` per [assets/BRANDING.md](assets/BRANDING.md).

### Also before final

- **Wire up offline browsing and the scrobble buffer** (see above). Both are small pieces of
  connecting work against code that already exists and is tested.
- **A pass over Windows and macOS on real hardware.**

## After 0.1.0

Not committed to, roughly in order of how likely they are:

- **OS keyring for the access token.** Today it lives in `config.toml` as plaintext, mitigated by a
  `0600` file mode, a startup warning if that has been widened, and redaction from every log line and
  `Debug` impl. A keyring backend is the proper fix.
- **A wider terminal-graphics matrix.** Artwork is exercised on Kitty, Ghostty, WezTerm and Konsole;
  Sixel terminals and image pass-through inside tmux need real testing.
- **Jellyfin.** The Emby client is a thin, well-isolated crate and the two APIs are close relatives.
  Worth investigating once Emby support is stable.
- **A `--print-config` flag** that emits a fully-commented config with every default filled in.
- **Richer search** — filters by year, genre and codec, rather than a single query line.

## Deliberately not planned

These were considered and rejected.

| | Why not |
| :-- | :-- |
| **Spectrum analyzer** | mpv exposes no cheap FFT tap. The options were an RMS-driven fake presented as a visualiser, or a second decode path costing real CPU for decoration. |
| **Crossfade** | mpv has no native crossfade. It needs a second handle, a volume-ramp task, role swapping and de-duplicated playback reporting — the most complex piece of the audio work, for a feature that works against the project's bit-perfect priority. Gapless is the only transition mode. |
| **1–5 star ratings** | Emby has no numeric rating. Its API takes a single boolean `Likes`, which is what favourites (`f`) uses. A star control that silently did nothing would be worse than no star control. |
| **Bit-perfect / exclusive-mode passthrough** | Removed during development. Reliable capability detection across ALSA, WASAPI Exclusive and CoreAudio turned out to be far more platform-specific guesswork than the benefit justified. |
| **Video** | loxia is a music client. |
