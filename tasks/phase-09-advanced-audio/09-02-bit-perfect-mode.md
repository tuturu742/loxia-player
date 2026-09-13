# 09-02 · Bit-perfect mode

**Phase:** 09 — Advanced audio · **Agent:** C · **Size:** M
**Prerequisites:** `09-01`
**Reference:** `docs/05-audio-engine.md` §4

## Goal
Lock the output device to the source format with no resampling and no DSP, and enforce the
exclusions that make that claim true.

## Files
- `crates/loxia-audio/src/device/*.rs` (extend)
- `crates/loxia-core/src/reducer/player.rs` (extend)

## Specification

**Per-OS settings** applied on `SetBitPerfect(true)`:

| OS | mpv settings |
| :-- | :-- |
| Linux | `audio-device = alsa/hw:X,Y` (raw `hw` only), `audio-exclusive = yes`, `af` cleared, `replaygain = no`, `audio-samplerate` unset so the source rate passes through |
| Windows | `audio-device = wasapi/{id}`, `audio-exclusive = yes` |
| macOS | `audio-device = coreaudio/{id}`, `audio-exclusive = yes` |

If the current device is not bit-perfect capable, pick the first capable one and toast the switch.
If none exists, refuse with `no bit-perfect capable output available` and leave the mode off.

**Exclusions, enforced in the reducer — not merely greyed out in the UI.** Enabling bit-perfect:
1. Saves EQ enabled-state, gains, ReplayGain mode, and quality profile into
   `PlayerState.pre_bitperfect`.
2. Forces EQ off, ReplayGain off, and `QualityProfile::Direct`.
3. Toasts what was disabled: `bit-perfect on — EQ and ReplayGain disabled, quality set to Direct`.

Disabling restores the saved snapshot exactly. Any DSP defeats the purpose of the mode, so this is a
correctness rule rather than a preference — a UI that let a user enable both would be lying about
what it is doing.

While bit-perfect is on, `SetEqGain`, `SetEqPreset`, `CycleReplayGain`, and `CycleQuality` are
refused with `disabled in bit-perfect mode`.

**Display.** The player bar's third line shows `Bit-Perfect` in `Accent` where the EQ field
normally sits (task `04-09` already reserves this).

**Format changes.** With an exclusive-mode device locked to one rate, a track at a different sample
rate causes mpv to reinitialise output, producing a brief gap. That is inherent to exclusive output
and is not worked around; note it in the settings help text.

## Acceptance
- `bit_perfect_disables_eq_and_replaygain_and_restores_on_off`
- `snapshot_restores_exact_previous_values`
- `quality_forced_to_direct_and_restored`
- `dsp_actions_refused_while_enabled` — table test over the four refused actions.
- `non_capable_device_switches_with_toast`
- `no_capable_device_refuses_and_stays_off`
- `player_bar_shows_bit_perfect` — snapshot.
- `linux_uses_raw_hw_not_plughw`

Manual, pasted into the PR (Linux): play a 24/96 FLAC with bit-perfect on and paste
`cat /proc/asound/card*/pcm*p/sub*/hw_params`, showing `rate: 96000` and a 24-bit format matching
the source.

## Done when
The global DoD in `tasks/README.md` is satisfied.
