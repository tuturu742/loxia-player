# 10-07 · Sleep timer modal

**Phase:** 10 — Polish · **Agent:** D · **Size:** S
**Prerequisites:** `09-05`, `03-07`
**Reference:** `design_overview` §3.6

## Goal
The `T` modal for arming the sleep timer.

## Files
- `crates/loxia-tui/src/modals/sleep_timer.rs`

## Specification

```
┌─ SLEEP & SHUTDOWN TIMER ──────────────────────────────────────────────┐
│  Stop playback after:                                                 │
│  ( ) 15 Minutes   ( ) 30 Minutes   ( ) 60 Minutes                     │
│  ( ) End of Current Track                                             │
│  (•) End of Active Queue (3 tracks remaining)                         │
│                                                                       │
│  Options:                                                             │
│  [X] Fade out audio smoothly over the last 10 seconds                 │
│  [ ] Close loxia when finished                                        │
│                                                                       │
│  [ Enter ] Start    [ d ] Disarm    [ Esc ] Cancel                    │
└───────────────────────────────────────────────────────────────────────┘
```

**Triggers are radio buttons** — exactly one is selected, rendered `( )` / `(•)`. The two options
below are independent checkboxes, `[ ]` / `[X]`. Using distinct glyphs for the two kinds of control
matters; the original wireframe used `[ ]` for both, which reads as though several triggers could be
active at once.

The `End of Active Queue` row shows the live remaining-entry count. `End of Current Track` shows the
remaining time on the current track. Both are derived at render time.

**Navigation:** `↑`/`↓` or `j`/`k` move between rows, `Space` or `Enter` toggles the focused
control, `Tab` moves between the trigger group and the options group.

**`Enter` on the footer arms the timer** and closes. When `End of Active Queue` is chosen while
`Repeat::All` is on, the toast from task `09-05` explains that repeat was disabled — the modal shows
this inline as a `Dim` note under the row, so the user sees it before committing rather than after.

**`d` disarms** an already-armed timer and closes. Cancelling with `Esc` leaves an armed timer
running — a user who opens the modal to check the remaining time and presses `Esc` must not lose
their timer.

**Already armed:** the title becomes `SLEEP TIMER — <remaining>` and the current trigger is
preselected.

**Nothing playing:** the two playback-relative triggers are `Dim` and unselectable, since neither
can ever fire.

## Acceptance
- `triggers_are_mutually_exclusive`
- `options_are_independent`
- `radio_and_checkbox_glyphs_differ`
- `queue_row_shows_live_remaining_count`
- `track_row_shows_remaining_time`
- `repeat_all_note_shown_inline_before_commit`
- `enter_arms_and_closes`
- `d_disarms`
- `esc_leaves_armed_timer_running`
- `already_armed_shows_remaining_in_title_and_preselects`
- `playback_triggers_disabled_when_nothing_playing`
- `sleep_timer_snapshot`, `_armed`, `_nothing_playing`

## Done when
The global DoD in `tasks/README.md` is satisfied.
