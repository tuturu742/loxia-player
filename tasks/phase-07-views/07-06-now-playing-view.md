# 07-06 · Now Playing view

**Phase:** 07 — Views · **Agent:** D · **Size:** M
**Prerequisites:** `06-05`, `04-09`
**Reference:** `docs/07-ui-spec.md` §8

## Goal
The two-pane Now Playing tab: the queue (or history) on the left, track detail and transport on the
right.

## Files
- `crates/loxia-tui/src/views/now_playing.rs`

## Specification

**Split.** Left pane 45 %, right pane 55 %, both bordered. Below 100 columns the right pane collapses
and the left takes the full width — the queue is the more useful of the two when space is scarce.

**Left pane — Queue.** One row per entry:
```
▶  3. Motion                      Boy Harsher        03:31  [album]
   4. Soft Pain                   Boy Harsher        04:02  [album]
```
- The current entry is marked `▶` and styled `Accent`; played entries are `Dim`.
- The trailing badge is the `QueueSource` (`album`, `playlist`, `mix`, `search`, `manual`).
- Numbering follows `play_order`, so it reflects actual play sequence when shuffled.
- Rows register `HitTarget::QueueEntry(QueueEntryId)`.
- `Enter` jumps to an entry, `x` removes it, `Ctrl+↑`/`Ctrl+↓` reorder.
- The pane auto-scrolls to keep the current entry visible when it changes, but **not** while the
  user is scrolling manually — track a `user_scrolled` flag cleared on track change.

**Left pane — History** (toggled with `H`):
```
• 14:20  Boy Harsher — Motion                              03:31
```
Newest first, timestamps in `HH:MM` from `played_at`. `a` re-queues the focused entry. History is
read-only otherwise.

The border title shows which sub-view is active and the key to swap:
`PLAY QUEUE  [{H}] History`, with the key from the keymap.

**Right pane**, top to bottom:
1. Track title (`Accent`, bold), artist, `Album: <name> (<year>) | Genre: <first genre>`.
2. A progress bar with timestamps — reuse `render_progress` from task `04-09`.
3. A transport row: `⏮  ⏯  ⏭    🔀 [ON]  🔁`, each registering `HitTarget::Transport`. Shuffle and
   repeat show their current state rather than being buttons with hidden state.
4. The lyrics pane (task `07-07`), filling the remaining height.

With nothing playing, the right pane shows a centred dim `nothing playing`.

## Acceptance
- `queue_numbering_follows_play_order_when_shuffled`
- `current_entry_marked_and_styled`
- `played_entries_dimmed`
- `source_badge_per_entry_type`
- `auto_scroll_follows_current_track`
- `auto_scroll_suppressed_while_user_scrolling`
- `history_toggle_and_border_hint`
- `history_is_newest_first`
- `requeue_from_history`
- `transport_registers_hit_targets`
- `right_pane_collapses_under_100_columns`
- `nothing_playing_state`
- `now_playing_snapshot_queue`, `_history`, `_narrow`, `_nothing_playing`

## Done when
The global DoD in `tasks/README.md` is satisfied.
