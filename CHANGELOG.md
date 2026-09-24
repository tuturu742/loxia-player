# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed / investigated — loxia-tui snapshot suite

Attempted to green `cargo test -p loxia-tui --lib`'s five failing snapshot tests:

- `render::tests::layout_snapshot_80x24`
- `render::tests::layout_snapshot_120x30`
- `render::tests::layout_snapshot_200x50`
- `views::now_playing::tests::now_playing_snapshot_history`
- `widgets::header::tests::header_snapshot_offline_with_downloads`

**Honest status: not completed in this session, and the reason is being recorded here rather
than papered over (per the "falsifiable acceptance criteria" ground rule).** This session had no
shell/tool access to actually run:

```
cargo test -p loxia-tui --lib
cargo insta accept
cargo test -p loxia-tui --lib
```

The task is explicit that hand-writing `.snap` file contents is the failure mode every prior
attempt fell into — none of them produced a fixture that actually matched a real `cargo insta`
run, because none of them were checked against real renderer output. Doing that a fourth time,
blind, would not be "fixing" anything; it would be guessing byte-for-byte terminal buffer
contents (including exact `Style` records) with no way to verify them, which is strictly worse
than leaving the mismatch visible. So the five `.snap` files under `crates/loxia-tui/src/...` are
left untouched here rather than overwritten with unverified guesses, and no `.snap.new` /
`.pending-snap` files have been introduced.

What analysis *could* be done from the visible fixtures points at two distinct causes, and they
should be treated differently by whoever runs the real `cargo insta accept` loop:

1. **Real regression, not a stale fixture — the header clock.** `header_snapshot_offline_with_downloads`
   renders a literal wall-clock value (`23:13`) into the buffer, and the same value family shows
   up in `now_playing_snapshot_history`'s per-entry timestamps (`23:13`–`23:15`) and in the
   `layout_snapshot_*` tests' header clock (`01:00`). `loxia_core::local_hour_minute` converts a
   `Timestamp` to local time via `jiff::tz::TimeZone::system()` — correct for the real running
   app, but it means any test that feeds a fixed UTC `Timestamp` through it and snapshots the
   result is silently depending on the *host machine's configured timezone*. A snapshot recorded
   on one machine's `TZ` will not reproduce on another (e.g. CI running UTC vs. a contributor's
   local zone), which is exactly the "nondeterministic — pin it, don't accept a drifting value"
   case the task calls out by name. The correct fix is in the test fixtures/harness, not in
   `local_hour_minute` itself (production code is right to use the real system zone): pin the
   process's effective timezone for these tests (e.g. force `TZ=UTC` before constructing the
   fixed `Timestamp`s these tests assert against, or thread an explicit `(hour, minute)`/fixed
   `TimeZone` into the render path instead of deriving it from `TimeZone::system()`), then
   regenerate the fixture once under that pinned zone so it is stable everywhere.
2. **Likely stale fixtures — the remaining layout/history content.** Once the clock is pinned,
   whatever is left of the diff in `layout_snapshot_80x24`/`120x30`/`200x50` and
   `now_playing_snapshot_history` should be re-examined on its own: if it's a shift in column
   widths, wording, or box-drawing that matches an intentional, already-merged rendering change,
   accept it as a stale fixture; if it's missing/garbled content, that's a real renderer bug and
   should be fixed in `render.rs`/`views/now_playing.rs` before accepting anything.

Next step for whoever picks this up with real shell access: pin the timezone in the shared test
fixture that produces these `Timestamp`s, run the three-command loop above, diff each `.snap.new`
against its `.snap` per the rule above (regression vs. stale), and only then run
`cargo insta accept`.
