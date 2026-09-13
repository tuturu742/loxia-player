# 10-05 · Device picker modal

**Phase:** 10 — Polish · **Agent:** D · **Size:** S
**Prerequisites:** `09-01`, `03-07`
**Reference:** `design_overview` §3.5

## Goal
The `O` modal for switching audio output devices mid-playback.

## Files
- `crates/loxia-tui/src/modals/device_picker.rs`

## Specification

```
┌─ SELECT AUDIO OUTPUT DEVICE ──────────────────────────────────────────┐
│  Active Engine: libmpv                                                │
│  ──────────────────────────────────────────────────────────────────── │
│  ALSA                                                                 │
│  ▶ • Direct Hardware Passthrough (hw:0,0)      [Bit-Perfect capable]  │
│  PipeWire                                                             │
│    • Default System Output Sink (Shared)                              │
│  PulseAudio                                                           │
│    • USB Headphones DAC                                               │
│                                                                       │
│  [ Enter ] Switch    [ Esc ] Cancel                                   │
└───────────────────────────────────────────────────────────────────────┘
```

- Devices are grouped by driver using `device::group_by_driver` (task `09-01`), with driver names as
  unselectable headers — reuse `section_header.rs` so cursor-skipping behaves as it does elsewhere.
- The currently active device is marked `•` filled; others hollow. The cursor is `▶`.
- `[Bit-Perfect capable]` annotates qualifying devices in `Dim`.
- `Loading` state while enumeration is in flight; an empty list shows
  `no output devices found`.
- `Enter` emits `Effect::Audio(SetDevice)` and closes. `Esc` closes with no change.
- Rows register `HitTarget::ModalField`; double-click selects.

**Refresh.** Opening always re-enumerates rather than showing a cached list — a user opens this
modal precisely because they just plugged something in.

**Failure feedback.** When the swap fails, the engine emits `DeviceUnavailable`; the reducer toasts
`could not switch to <name>` and leaves the previous device active. The modal is already closed by
then, so the toast is the only feedback — it must name the device.

The footer keys come from the keymap, not literals.

## Acceptance
- `devices_grouped_by_driver`
- `driver_headers_are_not_selectable`
- `active_device_marked`
- `bit_perfect_capable_annotated`
- `open_always_reenumerates`
- `loading_state_rendered`
- `empty_list_state`
- `enter_emits_set_device_and_closes`
- `esc_makes_no_change`
- `swap_failure_toasts_device_name`
- `device_picker_snapshot`, `_loading`, `_empty`

## Done when
The global DoD in `tasks/README.md` is satisfied.
