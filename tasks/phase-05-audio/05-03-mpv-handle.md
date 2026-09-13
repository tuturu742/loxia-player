# 05-03 · mpv handle

**Phase:** 05 — Audio MVP · **Agent:** C · **Size:** L
**Prerequisites:** `05-02`
**Reference:** `docs/05-audio-engine.md` §3

## Goal
Initialise libmpv2 with the correct options and implement the command side of `AudioBackend`. The
event side is task `05-04`.

## Files
- `crates/loxia-audio/src/mpv/handle.rs`, `props.rs`

## Specification

```
pub struct MpvEngine { .. }
impl MpvEngine {
    pub fn new(cfg: &AudioConfig) -> Result<MpvEngine, AudioError>;
}
impl AudioBackend for MpvEngine { .. }
```

**Initialisation options** — set exactly the table in `docs/05-audio-engine.md` §3. The ones that
are load-bearing rather than cosmetic:
- `terminal = no` — mpv writing to our TTY corrupts the alternate screen irrecoverably.
- `video = no`, `audio-display = no` — otherwise embedded cover art is decoded as a video track.
- `idle = yes`, `keep-open = no` — keeps the handle alive between tracks while still firing
  `end-file` so the queue can advance.
- `gapless-audio = yes`, `prefetch-playlist = yes` — the basis of gapless (task `06-06`).

**`props.rs`** holds typed constants for every property name and observation id. No string literal
for an mpv property may appear anywhere else; a typo in one is a silent no-op that is very hard to
find.

**Command implementation:**
| Command | mpv operation |
| :-- | :-- |
| `Load` | `loadfile <url> replace` with `start=<secs>` and per-file `http-header-fields` |
| `Preload` | `loadfile <url> append` |
| `Play` / `Pause` | set `pause` |
| `Stop` | `stop` |
| `Seek` | `seek <n> absolute` or `relative`; `Fraction` converts against `duration` |
| `SetVolume` | set `volume` |
| `SetMute` | set `mute` |
| `SetDevice` | set `audio-device` |
| `SetReplayGain` | set `replaygain` |
| `Shutdown` | signal the pump thread, then drop the handle |

**Missing library.** `libmpv2` fails at load time when libmpv is absent. Catch it and return
`AudioError::LibraryNotFound` with the platform hint from task `05-01`. `main` prints that hint and
exits non-zero — never a raw panic backtrace, which tells a user nothing actionable.

**Shutdown must be explicit and complete.** `Drop` signals the pump thread and joins it with a
2-second timeout. Without this the process hangs at exit, because the pump thread is blocked in
`wait_event`.

mpv's log messages are routed into `tracing` at the matching level under an `audio` span.

## Acceptance
Behind the non-default `mpv-tests` feature (not run in CI):
- `init_sets_documented_options` — read each option back after init and compare.
- `missing_library_returns_helpful_error` — simulate by pointing the loader at a bad path.
- `load_and_play_local_file` — a generated FLAC through the `null` audio output.
- `seek_absolute_and_relative`
- `shutdown_joins_pump_thread` — the process exits within 5 s (`timeout 5 cargo test`).

Always run:
- `property_names_are_centralised` — a grep test asserting no mpv property string literal appears
  outside `props.rs`.
- `fraction_seek_converts_against_duration`
- `command_mapping_table` — against a recording stub, asserting the mpv call for each command.

## Done when
The global DoD in `tasks/README.md` is satisfied.
