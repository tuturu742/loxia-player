# 03-08 · Runtime event loop

**Phase:** 03 — State machine · **Agent:** A · **Size:** M
**Prerequisites:** `03-06`, `01-06`
**Reference:** `docs/01-architecture.md` §4, `docs/04-state-and-input.md` §9

## Goal
Build the main loop: select over input, tick, and worker events; apply actions; dispatch effects;
redraw when dirty. Workers are stubs here — phases 04 and 05 fill them in.

## Files
- `crates/loxia-player/src/runtime.rs`, `dispatch.rs`, `workers/mod.rs`

## Specification

```
pub async fn run(state: AppState, guard: &mut TerminalGuard, workers: Workers) -> Result<()>;
```

The loop is a `tokio::select!` over three sources:
1. `crossterm::event::EventStream` → `input::to_action(..)` (task `03-09`)
2. `tokio::time::interval(100 ms)` → `Action::System(Tick(now))`
3. the Event channel receiver → `Event::into_action()`

Per iteration: apply the action, dispatch every returned effect, then redraw **only if
`state.dirty`**, resetting the flag afterwards.

**Render throttling.** Track the last draw instant; if fewer than 16 ms have elapsed, defer the
redraw to the next iteration rather than dropping it. Under a burst of held-key repeats this yields
a steady 60 fps instead of one draw per keystroke.

**Effect dispatch** (`dispatch.rs`) routes by group to per-worker unbounded senders. A closed
channel is logged at `error` and otherwise ignored — a dead worker must not take down the UI.
`Effect::Sys(Exit)` sets a flag that ends the loop after the current iteration.

**`Workers`** is a struct of senders plus the shared Event receiver. In this task each worker is a
task that logs the effect at `debug` and, for the ones that need a reply, sends back a plausible
stub `Event`, so the loop is exercisable end to end before phases 04 and 05 land.

**Tick responsibilities** are the reducer's, not the loop's — the loop only supplies the timestamp.
The full list is in `docs/04-state-and-input.md` §9; implement the two that need no other phase:
expiring toasts and expiring `pending_chord`.

**Shutdown:** on `should_quit`, emit a final `PersistSession` effect, await worker drain with a
2-second timeout, restore the terminal, and return. A hung worker must not prevent exit.

## Acceptance
- `loop_applies_action_and_dispatches_effect` — a headless test with a `TestBackend` and scripted
  actions.
- `redraw_only_when_dirty` — a counting backend; an action that changes nothing draws zero frames.
- `render_throttled_to_16ms` — 100 actions in a burst produce far fewer than 100 draws, and the
  final state is drawn.
- `closed_worker_channel_does_not_panic`
- `quit_drains_and_exits`
- `hung_worker_does_not_block_exit` — a worker that never drains; the loop exits within 3 s.
- `toasts_expire_on_tick`
- `pending_chord_expires_after_one_second`
- Manual: `cargo run -p loxia-player` shows a debug state dump, responds to `j`/`k`, and idles under 1 % CPU
  (`top` reading pasted into the PR).

## Done when
The global DoD in `tasks/README.md` is satisfied.
