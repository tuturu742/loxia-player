# 10-06 · Equalizer modal

**Phase:** 10 — Polish · **Agent:** D · **Size:** M
**Prerequisites:** `09-03`, `10-04`
**Reference:** `design_overview` §3.8

## Goal
The `e` modal: a 10-band graphic equalizer with live preview, presets, bypass, and drag support.

## Files
- `crates/loxia-tui/src/modals/equalizer.rs`

## Specification

```
┌─ EQUALIZER [Preset: Darkwave / EBM] ─────────────────── [Status: ACTIVE] ─┐
│  +12dB ┤       ██                          ██                            │
│   +6dB ┤   ██  ██  ██                  ██  ██  ██                        │
│    0dB ┼───██──██──██──██──██──██──██──██──██──██──────────────────────── │
│   -6dB ┤               ██  ██  ██  ██                                    │
│  -12dB ┤                                                                 │
│        └─┬───┬───┬───┬───┬───┬───┬───┬───┬───┬─                          │
│         31  63 125 250 500  1k  2k  4k  8k 16k                           │
│  [←/→] Band  [↑/↓] ±0.5dB  [p] Presets  [b] Bypass  [Esc] Close           │
└──────────────────────────────────────────────────────────────────────────┘
```

**Bars** grow up or down from the 0 dB centre line, using vertical block glyphs for sub-row
precision (`▁▂▃▄▅▆▇█`); `ascii_only` uses `#`. The selected band's column is highlighted in
`Accent` including its frequency label.

**Live preview.** `↑`/`↓` update `draft_gains` and emit `Effect::Audio(SetEq)` immediately, so
changes are heard while adjusting. This is why task `09-03` installs the filter chain once — without
that, every keypress would restart playback.

**Commit and cancel.** `Enter` commits to config and closes. `Esc` restores `gains_at_open`, emits
`SetEq` to undo the preview, and closes. A user who experiments and then cancels must get their
original sound back, not keep whatever they last previewed.

**`p`** cycles presets, factory then custom, updating all ten bars at once.
**`b`** toggles bypass; the status in the title becomes `[Status: BYPASSED]` and the bars render in
`Dim` while retaining their values.

**Bit-perfect.** Opening the modal while bit-perfect is on shows the bars in `Dim` with
`[Status: DISABLED — bit-perfect mode]` in the title, and all adjustment keys are refused with the
toast from task `09-02`. Hiding the modal entirely would be more confusing than showing why it is
unavailable.

**Mouse:** each band registers `HitTarget::EqBand`; click sets the gain from the y position and drag
adjusts continuously (task `10-04`).

Height degrades gracefully: below 16 rows the ±6 dB gridlines are dropped before the bars are.

## Acceptance
- `bars_render_from_centre_line`
- `selected_band_highlighted_with_label`
- `up_down_adjusts_by_half_db_and_emits`
- `gain_clamped_at_twelve_db`
- `esc_restores_gains_and_emits_undo`
- `enter_commits_to_config`
- `p_cycles_factory_then_custom_presets`
- `bypass_dims_bars_and_updates_title`
- `bit_perfect_shows_disabled_status_and_refuses_input`
- `eq_bands_register_hit_targets`
- `short_terminal_drops_gridlines_first`
- `ascii_only_bars_have_no_multibyte`
- `equalizer_snapshot`, `_bypassed`, `_bit_perfect_disabled`, `_short`

## Done when
The global DoD in `tasks/README.md` is satisfied.
