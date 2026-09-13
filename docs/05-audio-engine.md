# 05 — Audio Engine (`loxia-audio`)

## 1. Backend

**`libmpv2` 6.0** — dynamic linking against system libmpv (Linux, macOS) or the bundled
`mpv-1.dll` (Windows). It targets the libmpv 2.0 client API, i.e. mpv ≥ 0.35, which is what every
current distribution ships.

mpv gives us, correctly and for free: FLAC/ALAC/Opus decode, HTTP streaming with custom headers,
gapless playback, ReplayGain, a filter chain for EQ, per-OS exclusive output modes, and device
enumeration. Reimplementing that on `cpal` + `symphonia` is a multi-month detour with a worse
result.

Everything sits behind a trait so the rest of the workspace never touches mpv directly:

```
pub trait AudioBackend: Send {
    fn send(&self, cmd: AudioCommand) -> Result<(), AudioError>;
    fn subscribe(&self) -> mpsc::UnboundedReceiver<AudioEvent>;
}
```
`MpvEngine` is the real implementation. `MockEngine` is a deterministic fake driven by a virtual
clock, used by the default test suite and by `loxia-player --no-audio`. **The whole test suite must pass
on a machine with no mpv installed.**

## 2. Command and event surface

### `AudioCommand`
`Load { url, headers, start_at, gain_db }`, `Preload { url, headers, gain_db }`, `Play`, `Pause`,
`Stop`, `Seek(SeekTarget)`, `SetVolume(u8)`, `SetMute(bool)`, `SetEq(Option<EqCurve>)`,
`SetReplayGain(ReplayGainMode)`, `SetDevice(String)`, `EnumerateDevices`,
`Shutdown`

### `AudioEvent`
`StatusChanged(PlayStatus)`, `Position { secs, duration }`, `Format(AudioFormat)`,
`TrackEnded { natural: bool }`, `Devices(Vec<AudioDevice>)`, `Buffering(u8)`, `Error(AudioError)`

Events fire only on **real** changes. Position is throttled to 4 Hz; format is emitted once per
load. Emitting a position event per mpv callback would flood the channel and the render loop.

## 3. mpv initialisation (`mpv/handle.rs`)

| Option | Value | Reason |
| :-- | :-- | :-- |
| `video` | `no` | audio-only client |
| `audio-display` | `no` | do not decode cover art as a video track |
| `terminal` | `no` | mpv must never write to our TTY |
| `msg-level` | `all=warn` | routed into `tracing` through the mpv log handler |
| `idle` | `yes` | keep the handle alive between tracks |
| `keep-open` | `no` | let `end-file` fire so the queue advances |
| `gapless-audio` | `yes` | the 0 ms transition requirement |
| `prefetch-playlist` | `yes` | preloads the next playlist entry |
| `cache` | `yes` | network resilience |
| `cache-secs` | from `audio.buffer_size_ms` | |
| `audio-client-name` | `loxia-player` | shows correctly in PipeWire/PulseAudio mixers |
| `replaygain` | `album` / `track` / `no` | from config |
| `replaygain-preamp` | `audio.replaygain_preamp_db` | |
| `replaygain-clip` | `yes` | prevents clipping on positive gain |
| `volume-max` | `100` | |
| `http-header-fields` | joined custom headers | **required for reverse-proxy deployments** |
| `user-agent` | `loxia/<version>` | |

**Observed properties** (`observe_property` — the app is event-driven, it never polls):
`time-pos`, `duration`, `pause`, `core-idle`, `eof-reached`, `audio-params`, `audio-bitrate`,
`audio-device-list`, `demuxer-cache-state`, `volume`, `mute`, `path`, `playlist-pos`.

**Threading.** The mpv handle is owned by one dedicated OS thread that blocks in `wait_event` and
forwards results into the event channel. It must be shut down explicitly on `Shutdown` — otherwise
the process hangs at exit. A test asserts the process terminates within 5 seconds of a quit.

## 4. Devices

### Enumeration
Read `audio-device-list` → `Vec<AudioDevice { id, description, driver }>`, where `id` looks like
`alsa/hw:0,0`, `pipewire/…`, `pulse/…`, `wasapi/…`, or `coreaudio/…`. The picker groups by driver.
Refresh on modal open and whenever mpv reports a device-list change.

### Hot-swap (`O`)
Set `audio-device`. mpv reinitialises output, so a short gap is expected. Read `time-pos` first; on
error, fall back to the previous device and raise a toast. Never leave the user with silence and no
explanation.

## 5. Equalizer (`eq.rs`)

Ten ISO bands — 31, 63, 125, 250, 500, 1 k, 2 k, 4 k, 8 k, 16 k Hz — with a ±12 dB range in 0.5 dB
steps.

**Install the chain once, then mutate gains.** At load time, install a full 10-band `anequalizer`
filter with every gain at 0 dB and a stable label. Subsequent changes use `af-command` to set
individual band gains. Rebuilding the `af` chain on every keypress makes mpv restart playback,
which is audible and unacceptable while the modal is open.

Factory presets live in `assets/eq_presets.toml`, embedded with `include_str!`: `flat`,
`darkwave_ebm`, `bass_boost`, `vocal`, `acoustic`, `night_listening_warm`, `loudness`, `classical`.
User presets append to `config.equalizer.custom_presets`.

The modal renders `draft_gains` and applies changes live so they can be heard. `Esc` restores
`gains_at_open`; closing with `Enter` commits to config.

## 6. ReplayGain (`replaygain.rs`)

`Album | Track | Off` maps directly to mpv's `replaygain` property, with `replaygain-preamp` and
`replaygain-clip=yes` alongside.

When a track carries no ReplayGain tags, fall back to Emby's `normalizationGain` from the
`MediaSources` payload, applied as `gain_db` on the `Load` command. `PlayerState.applied_gain_db`
records which path was taken and the inspector displays it, so the behaviour is never mysterious.

## 7. Gapless (`gapless.rs`)

**Gapless is the only transition mode.** Crossfade was cut (`12-decisions.md` §3).

Implementation: when a track starts, append the next queue entry's URL to the mpv playlist and let
`prefetch-playlist` plus `gapless-audio` do the work. Track advancement is detected through the
`playlist-pos` / `path` property change, which emits `TrackEnded { natural: true }`.

The reducer emits `Effect::Audio(Preload)` on **any queue mutation that changes the entry at
`position + 1`** — appending, removing, reordering, shuffling, or skipping. Forgetting one of those
paths is the standard way gapless silently stops working; there is a test per mutation.

Measured target: gap < 20 ms between tracks of identical format.

## 8. Error handling

| Failure | Behaviour |
| :-- | :-- |
| libmpv missing at startup | Fatal, but with a per-OS install hint (`brew install mpv`, `pacman -S mpv`, "the Windows installer bundles mpv-1.dll") and a clean non-zero exit — never a raw panic backtrace |
| Load fails (404 / 403) | `AudioEvent::Error`, mark the entry `Unavailable`, auto-skip, toast once |
| Device disappears (USB unplug) | mpv error → fall back to `auto`, toast, keep playing |
| Decode error mid-track | Skip to the next entry, log at `warn` |

## 9. Tests

- **Unit:** EQ curve → filter-string snapshot; ReplayGain mode mapping; the exclusion
  matrix; device-id parsing for all five driver prefixes.
- **`MockEngine`:** drives every reducer-level playback test deterministically, including
  `TrackEnded` sequencing, stalls, load failures, and device disappearance.
- **Real-mpv integration**, behind the non-default `mpv-tests` feature: play a generated 3-second
  FLAC through the `null` audio output and assert `Format`, monotonic `Position`, and
  `TrackEnded { natural: true }`. Not run in default CI.
