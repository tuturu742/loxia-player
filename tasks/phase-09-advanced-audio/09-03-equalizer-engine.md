# 09-03 · Equalizer engine

**Phase:** 09 — Advanced audio · **Agent:** C · **Size:** L
**Prerequisites:** `05-06`
**Reference:** `docs/05-audio-engine.md` §5

## Goal
A 10-band graphic equalizer whose gain changes apply live, without restarting playback.

## Files
- `crates/loxia-audio/src/eq.rs`, `mpv/filters.rs`
- `assets/eq_presets.toml`

## Specification

Bands are the ISO centres in `EQ_BANDS_HZ` (task `05-01`); the range is ±12 dB in 0.5 dB steps.

**Install the chain once, then mutate gains.** At engine init, install a full 10-band `anequalizer`
filter with a stable label and every gain at 0 dB. Gain changes then use `af-command` against that
label. Rebuilding the `af` chain per keypress makes mpv restart playback — an audible stutter every
time the user nudges a slider, which makes the modal unusable.

```
pub fn filter_string(curve: &EqCurve) -> String;   // the initial install
pub fn band_command(band: usize, gain_db: f32) -> (String, String);  // (name, args) for af-command
pub fn clamp_gain(db: f32) -> f32;                 // ±12, snapped to 0.5
```

Each band is one `anequalizer` entry: `c0 f=31 w=<width> g=<gain> t=1`, with width set to one
octave. Band gains are clamped and snapped before being sent.

**Bypass** (`b` in the modal) sets every band to 0 dB without uninstalling the chain, so toggling
bypass is instantaneous and reversible.

**Presets.** `assets/eq_presets.toml`, embedded with `include_str!`, defining the eight names in
`loxia_core::config::schema::FACTORY_EQ_PRESET_NAMES` (added by task `01-02`, which needed the list
before this task exists to check custom-preset name collisions — reuse it, don't redefine it, or
the two lists will drift). Each is a name plus ten gains. User presets come from
`config.equalizer.custom_presets` and are appended; a user preset whose name collides with a factory
one was already renamed by config validation (task `01-02`).

**Reducer.** `SetEqGain { band, db }` updates `player.eq.gains` and emits `Effect::Audio(SetEq)`.
`SetEqPreset` replaces all ten. `ToggleEqBypass` flips the flag. All three are refused while
bit-perfect is on (task `09-02`).

Enabling the EQ from `enabled = false` installs the chain and applies the active preset.

## Acceptance
- `filter_string_snapshot` — `insta` snapshots for flat, a boost, and a cut.
- `band_command_snapshot`
- `gain_is_clamped_to_plus_minus_twelve`
- `gain_snaps_to_half_db`
- `all_factory_presets_parse`
- `factory_presets_have_ten_gains_in_range`
- `bypass_zeroes_without_uninstalling`
- `bypass_is_reversible`
- `custom_presets_appended_after_factory`
- `eq_actions_refused_in_bit_perfect_mode`
- `enabling_eq_applies_active_preset`

Behind `mpv-tests`:
- `gain_change_does_not_restart_playback` — play a file, change a gain at t=1 s, and assert the
  position continues monotonically with no `Load` and no status change.

Manual, pasted into the PR: sweep the 63 Hz band from −12 to +12 dB during playback and confirm the
change is audible and continuous with no stutter.

## Done when
The global DoD in `tasks/README.md` is satisfied.
