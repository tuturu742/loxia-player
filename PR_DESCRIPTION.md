# loxia-tui snapshot suite: status and per-test classification

## Status (read this first)

This session, like the one before it, has no working shell/`cargo`/`cargo insta` access — there
is no way from here to actually execute:

```
cargo test -p loxia-tui --lib
cargo insta accept
cargo test -p loxia-tui --lib
```

and observe a real pass/fail result, or to see the full, untruncated contents of
`loxia-core::local_hour_minute`, `loxia_core::state::AppState`, or the complete bodies of
`crates/loxia-tui/src/render.rs`, `crates/loxia-tui/src/widgets/header.rs`, and
`crates/loxia-tui/src/views/now_playing.rs` (every one of those was supplied to this session
truncated mid-function/mid-sentence).

The task is explicit, twice over, that hand-writing `.snap` file contents is the exact failure
mode every prior attempt fell into, and the reviewer's own instructions name the correct response
to a genuine "cannot run cargo" blocker: **escalate it**, rather than land guessed `.snap` files
or blind edits to source files whose full, current contents aren't visible to the editor making
the change. Editing `local_hour_minute`'s call sites or `AppState`'s shape without seeing their
full current definitions risks introducing a second, silent regression on top of the one this
item exists to fix — worse than leaving the branch honestly incomplete.

**What this PR does do:** restores the `CHANGELOG.md` regression from the previous attempt (the
deleted `### Added` entries are back, and the investigation notes have been moved out of the
changelog and into this document, where the per-test classification belongs). **What it does not
do:** touch `render.rs`, `views/now_playing.rs`, `widgets/header.rs`, `loxia-core`'s clock
helper, or any `.snap` file — because doing so blind, in an environment that cannot verify the
result against a real `cargo test` run, is the thing this whole work item exists to stop
happening a third time.

**Required next step, with a working `cargo`/`cargo insta` loop:**

1. Add a `TimeZone`-taking variant of `loxia_core::local_hour_minute` (e.g.
   `local_hour_minute_in(ts: Timestamp, tz: &TimeZone) -> (u8, u8)`), keeping the existing
   `local_hour_minute(ts)` calling `TimeZone::system()` internally for production callers.
2. Thread an explicit `TimeZone` into the render path (via `AppState` or a render context) so
   `widgets/header.rs`'s clock, `views/now_playing.rs`'s history-entry timestamps, and
   `render.rs`'s own header render all derive HH:MM from the same explicit zone. Production
   passes `TimeZone::system()`; the `draw_at`/`render_at` test helpers pass `TimeZone::UTC`.
3. Run `cargo test -p loxia-tui --lib`, inspect every `.snap.new` against its `.snap` by eye —
   if a diff is a real rendering defect (garbled/missing content, not just a shifted clock value
   or incidental width change), fix the renderer instead of accepting it.
4. `cargo insta accept` (or copy each verified `.snap.new` over its `.snap`), rerun until 0
   failures, and confirm no `.snap.new`/`.pending-snap` files remain.

## Per-test classification (from static inspection of the fixtures available in this session)

- **`widgets::header::tests::header_snapshot_offline_with_downloads`** — *stale/nondeterministic
  fixture, clock pinned; not a renderer regression.* The committed snapshot renders a literal
  wall-clock value (`23:13`) produced by `format_clock` → `loxia_core::local_hour_minute`, which
  the header module's own doc comment says reads `state.clock` (good — not `Timestamp::now()`),
  but the conversion to `(hour, minute)` goes through `jiff::tz::TimeZone::system()`. Two runs of
  the same fixed `Timestamp` on hosts (or CI runners) configured with different system timezones
  render different HH:MM text, which is exactly what makes this snapshot fail non-reproducibly
  rather than deterministically. This is nondeterminism, not wrong output — the rest of the one
  visible row (`OFFLINE`, `↓2`, badge ordering/styling, `[?]`) matches the spec's documented
  layout and role colours. Fix: pin the zone explicitly (see above), then re-accept.

- **`views::now_playing::tests::now_playing_snapshot_history`** — *stale/nondeterministic
  fixture, clock pinned; not a renderer regression.* Same root cause: each history row's leading
  timestamp (`23:15`, `23:14`, `23:13`) is derived the same way from a fixed `Timestamp`, so it is
  host-timezone-dependent. The content around it — track/artist text, duration column, queue/
  history tab labelling, right-pane album art placeholder and metadata — reads as intentional,
  correctly laid out content, not corruption. Fix: pin the zone, re-accept once verified.

- **`render::tests::layout_snapshot_80x24`**, **`layout_snapshot_120x30`**,
  **`layout_snapshot_200x50`** — *stale/nondeterministic fixture, clock pinned; not a renderer
  regression.* All three embed the same header clock (`01:00` in the committed fixtures) via the
  same `header::render` path, so they fail for the identical reason as the two tests above: it is
  the one non-content-bearing cell in an otherwise-correct full-frame layout (sidebar labels,
  Miller/queue placeholders, player bar, box-drawing borders all match the documented 07-ui-spec
  layout at each width). Fix: pin the zone, re-accept once verified — no renderer code change is
  otherwise expected for these three.

## Summary

Of the five, **zero** are classified here as genuine rendering regressions requiring a
`render.rs`/`views/now_playing.rs`/`widgets/header.rs` logic fix — all five point at the same
single root cause (`local_hour_minute` reading `TimeZone::system()`), which is nondeterminism,
not wrong output. No renderer code was changed in this PR, because the fix (threading an explicit
`TimeZone`) and the verification (running `cargo test` / `cargo insta accept`) both require a
working shell, which this session does not have. That absence is being escalated here rather than
worked around by hand-writing `.snap` files or editing source this session cannot see in full —
consistent with the reviewer's own instruction for a genuine "cannot run cargo" blocker.
