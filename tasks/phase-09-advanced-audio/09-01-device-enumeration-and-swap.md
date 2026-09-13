# 09-01 · Device enumeration and hot-swap

**Phase:** 09 — Advanced audio · **Agent:** C · **Size:** M
**Prerequisites:** `05-06`
**Reference:** `docs/05-audio-engine.md` §4

## Goal
List the system's audio outputs and switch between them mid-track without losing position.

## Files
- `crates/loxia-audio/src/device/mod.rs`, `linux.rs`, `windows.rs`, `macos.rs`

## Specification

**Enumeration.** Read mpv's `audio-device-list` property → `Vec<AudioDevice { id, description,
driver }>`. Ids arrive as `alsa/hw:0,0`, `pipewire/…`, `pulse/…`, `wasapi/{guid}`, `coreaudio/…`;
split on the first `/`. Refresh on modal open and whenever mpv reports the list changed (a USB DAC
being plugged in).

**Grouping and labelling** (`device/mod.rs`):
```
pub fn group_by_driver(devices: &[AudioDevice]) -> Vec<(String, Vec<AudioDevice>)>;
pub fn label(d: &AudioDevice) -> String;
```
Driver order in the picker is **most capable first**: ALSA, WASAPI, CoreAudio, then PipeWire, Pulse.
A device usable for bit-perfect output is annotated `[Bit-Perfect capable]`:
- Linux: an `alsa/hw:X,Y` id — **not** `default` or `plughw`, which resample.
- Windows: any WASAPI device, which supports exclusive mode.
- macOS: any CoreAudio device.

The per-OS modules provide only `is_bitperfect_capable` and any driver-name prettifying. Everything
else is shared — `#[cfg]` blocks are confined to these three files
(`docs/README.md` rule 5).

**Hot-swap.** On `SetDevice`:
1. Read `time-pos`.
2. Set `audio-device`.
3. mpv reinitialises output; a short gap is expected and is not an error.
4. On failure, restore the previous device and emit `Error(DeviceUnavailable)` so the UI can toast.

Never leave the user with silence and no explanation — that is the failure mode that makes people
think the app is broken.

**Device disappearance** (unplugged mid-playback): mpv emits an error; fall back to `auto`, keep
playing, and toast `output device removed — switched to default`.

**Persistence.** A successful swap writes `audio.device_id` and `audio.output_driver` to config,
debounced, so the choice survives a restart.

## Acceptance
Always run:
- `device_id_parsing_table` — all five prefixes plus a bare id with no `/`.
- `group_by_driver_orders_most_capable_first`
- `bitperfect_capable_detection` — three `#[cfg]`-gated table tests: `alsa/hw:0,0` yes,
  `alsa/default` no, `alsa/plughw:0,0` no, any `wasapi/*` yes, any `coreaudio/*` yes.
- `label_is_human_readable`
- `cfg_blocks_confined_to_device_modules` — a grep test over `crates/loxia-audio/src`.

Behind `mpv-tests`:
- `enumerate_returns_at_least_one_device`
- `swap_preserves_position`
- `swap_to_invalid_device_restores_previous`

Manual, pasted into the PR: switch devices mid-track on Linux and on one of macOS/Windows; unplug a
USB output during playback and confirm the fallback toast.

## Done when
The global DoD in `tasks/README.md` is satisfied.
