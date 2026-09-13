# 05-04 · mpv event pump

**Phase:** 05 — Audio MVP · **Agent:** C · **Size:** M
**Prerequisites:** `05-03`
**Reference:** `docs/05-audio-engine.md` §§2–3

## Goal
Observe mpv properties on a dedicated thread and translate them into `AudioEvent`s. This is how the
app learns anything about playback; there is no polling anywhere.

## Files
- `crates/loxia-audio/src/mpv/handle.rs` (extend)

## Specification

**Thread.** One dedicated OS thread — not a tokio task — blocking in `wait_event(-1)`. libmpv's
event API is blocking and not async-aware, so occupying a runtime worker with it would starve the
executor. It forwards into a `std::sync::mpsc`, bridged to the tokio channel returned by
`subscribe()`.

**Observed properties** are exactly the list in `docs/05-audio-engine.md` §3.

**Property → event mapping:**
| Property / mpv event | `AudioEvent` |
| :-- | :-- |
| `time-pos`, `duration` | `Position { secs, duration }` — **throttled to 4 Hz** |
| `pause`, `core-idle` | `StatusChanged(Playing / Paused / Buffering)` |
| `audio-params`, `audio-bitrate` | `Format(AudioFormat)` — emitted **once per load** |
| `end-file` with reason `eof` | `TrackEnded { natural: true }` |
| `end-file` with reason `stop`/`quit` | `TrackEnded { natural: false }` |
| `end-file` with reason `error` | `Error(AudioError::Load { .. })` |
| `playlist-pos` change | `TrackEnded { natural: true }` — the gapless advance path |
| `audio-device-list` | `Devices(Vec<AudioDevice>)` |
| `demuxer-cache-state` | `Buffering(percent)` |
| `volume`, `mute` | `StatusChanged` carrying the new values |

**Emit only on real change.** Compare against the last emitted value and skip duplicates. mpv fires
`time-pos` far more often than 4 Hz, and forwarding every one floods the channel and the render
loop for no visible benefit.

**Format extraction** from `audio-params`: `format` (`s16`, `s32`, `float`…) → bit depth,
`samplerate` → `sample_rate_hz`, `channel-count` → `channels`, plus `audio-bitrate`. Codec comes
from the `file-format`/`audio-codec-name` property. An unrecognised value maps to `Codec::Other`
rather than failing — the format line is informational.

**Device id parsing:** mpv reports `alsa/hw:0,0`, `pulse/…`, `pipewire/…`, `wasapi/{guid}`,
`coreaudio/…`. Split on the first `/` into driver and id; a string with no `/` uses driver `auto`.

**Shutdown:** the pump exits on `Shutdown` or when `wait_event` returns the mpv shutdown event, then
drops its sender so `subscribe()`'s receiver closes cleanly.

## Acceptance
Always run:
- `position_events_throttled_to_4hz` — feed 100 synthetic `time-pos` updates across 1 s of
  simulated time through the translation function; assert 4 events out.
- `duplicate_property_values_do_not_emit`
- `end_file_reason_mapping` — table test over `eof`, `stop`, `quit`, `error`.
- `format_extraction_table` — s16/44100/2, s32/96000/2, float/48000/2.
- `unknown_codec_maps_to_other`
- `device_id_parsing_table` — all five driver prefixes plus a bare id.

Behind `mpv-tests`:
- `real_playback_emits_format_then_positions_then_ended` — play a 3-second FLAC through `null` and
  assert the full event sequence.

## Done when
The global DoD in `tasks/README.md` is satisfied.
