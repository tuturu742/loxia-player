# 04-09 · Player bar

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** M
**Prerequisites:** `04-03`, `04-04`
**Reference:** `docs/07-ui-spec.md` §7

## Goal
The persistent three-line player bar with the progress bar and audio inspector line. Always visible,
so it must degrade gracefully at every width.

## Files
- `crates/loxia-tui/src/widgets/player_bar.rs`, `progress.rs`

## Specification

```
▶ <title> — <artist> (<album>)                              [availability glyph]
<mm:ss> [progress] <mm:ss>
🎚 <codec> <depth>/<rate> │ Bitrate: <kbps> │ Output: <driver> (<device>) │ EQ: <state>
```

**Line 1.** Status glyph from `player.status`: `▶` playing, `⏸` paused, `⏹` stopped, `⋯` buffering,
`◌` loading. When stopped with a restored session, append `resumed at <mm:ss>` in `Dim`. When
nothing is loaded, show a dim `nothing playing`.

**Line 2 — progress** (`progress.rs`):
```
pub fn render_progress(f: &mut Frame, area: Rect, pos: Duration, dur: Duration,
                       buffered: Option<f32>, theme: &Theme, hits: &mut HitMap);
```
- Sub-cell precision with the partial blocks `▏▎▍▌▋▊▉█`; `ascii_only` uses `#` and `-`.
- The buffered fraction renders in the unplayed region with a distinct glyph.
- A zero duration renders an empty bar and `--:--`, never a division by zero.
- Registers `HitTarget::SeekBar` over the bar area only, not the timestamps.

**Line 3 — collapse order.** As width shrinks, drop fields right to left: `EQ` → `Output` →
`Bitrate` → the depth/rate portion. The codec name is never dropped.
`EQ` shows `Active [<preset>]`, `Bypassed`, or `Off`. When bit-perfect is engaged, line 3 shows
`Bit-Perfect` in `Accent` in place of the EQ field, since DSP is necessarily off.

Time formatting: `mm:ss` under an hour, `h:mm:ss` at or above.

The widget reads `player.position` from state and never extrapolates from a wall clock.

## Acceptance
- `player_bar_snapshot_playing`, `_paused`, `_stopped`, `_buffering`, `_bit_perfect`
- `progress_subcell_precision` — 50 % of a 10-cell bar renders 5 full blocks; 55 % renders 5 plus a
  partial.
- `progress_zero_duration_does_not_panic`
- `progress_registers_seekbar_hit_target`
- `buffered_region_uses_distinct_glyph`
- `line3_collapse_order` — table test at widths 200, 140, 110, 90, 80.
- `codec_never_dropped`
- `time_format_switches_at_one_hour`
- `ascii_only_progress_has_no_multibyte`
- `resumed_hint_shown_after_session_restore`

## Done when
The global DoD in `tasks/README.md` is satisfied.
