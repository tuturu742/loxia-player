# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Investigation notes — five failing loxia-tui snapshot tests (rework of pyr/b017fd72-2)

This entry documents the verdict for each of the five named tests, as required by the task, and
explains why this pass still leaves the `crates/loxia-tui` source and `.snap` files themselves
untouched.

**Per-test verdict:**

1. `render::tests::layout_snapshot_80x24` — suspected **(b) regression**, cascading from header.rs
   (see below): `render::draw` always paints `header::render` first, so any wrong text produced by
   the header widget shows up verbatim inside every full-frame layout snapshot, at every terminal
   size.
2. `render::tests::layout_snapshot_120x30` — suspected **(b) regression**, same cascade as above.
3. `render::tests::layout_snapshot_200x50` — suspected **(b) regression**, same cascade as above.
4. `widgets::header::tests::header_snapshot_offline_with_downloads` — suspected **(b) regression**.
   `crates/loxia-tui/src/widgets/header.rs` defines:
   ```rust
   const HELP_LABEL: &str = "[?]";
   ```
   and its own doc comment says the mouse-reachable help button is "labelled with the key that does
   the same thing from the keyboard, so the button teaches the binding." A literal `"[?]"` constant
   is exactly the pattern `CONTRIBUTING.md`'s hard rule forbids ("Never hardcode a keybinding in UI
   text — render through `KeyMap::hint_for(ActionId)`"), and the task brief for this rework calls
   that rule out by name as something to check. The header's stored `.snap` most plausibly still
   holds the dynamic hint text (e.g. whatever `KeyMap::hint_for(ActionId::Help)` currently resolves
   to for the default keymap), while the live code now renders the hardcoded literal — a mismatch
   that would fail the snapshot even though nothing about the offline/download badges themselves is
   wrong.
5. `views::now_playing::tests::now_playing_snapshot_history` — **undetermined in this pass**; no
   single suspicious pattern (hardcoded text, wall-clock read, etc.) is visible in the portion of
   `views/now_playing.rs` inspected. Needs its own diff read against the stored `.snap` to classify
   as (a), (b), or (c).

**Why no `.rs`/`.snap` file is changed in this commit:**

The fix implied by finding 4 above — replacing `HELP_LABEL` with a call through
`KeyMap::hint_for(ActionId::Help)` (formatted the same way the rest of `header.rs`'s badges are, and
threaded through the same `state`/`hits` plumbing the existing help-button code already uses) — needs
to be checked against the *complete* current source of `widgets/header.rs` (its render function,
imports, and its own test module), plus the actual stored `.snap` bytes, so the replacement text
matches exactly and the file still compiles and satisfies `header.rs`'s own existing tests (e.g.
whatever asserts the help button's hit-target offset). Only a truncated excerpt of `header.rs`,
`render.rs`, and `now_playing.rs` was available while preparing this pass, without the tooling to run
`cargo insta test -p loxia-tui --review`, `git log --follow`, or `cargo test -p loxia-tui` to confirm
the exact stored snapshot text and the exact commit that introduced the hardcoded label.

Editing `header.rs` blind, from a partial excerpt, risks either breaking compilation (missing
context for other call sites of `HELP_LABEL`/the help-button hit target) or hand-typing snapshot
content that does not actually match what `ratatui`'s buffer produces — which would trade one set of
failing tests for another, and would still leave `now_playing_snapshot_history` unclassified.

**Next step for whoever has full repository and test access:**

1. Run `cargo insta test -p loxia-tui --review` (or read the five `.snap.new` files) to get the
   exact diff for each of the five tests.
2. For `header_snapshot_offline_with_downloads` and the three `render::layout_snapshot_*` tests,
   confirm the diff is exactly the help-button text (hardcoded `[?]` vs. the keymap-derived hint).
   If so, change `widgets/header.rs` to render the help button via
   `state.keymap.hint_for(ActionId::Help)` (bracketed the same way `HELP_LABEL` was), remove the
   `HELP_LABEL` constant, and leave the four `.snap` files untouched — this is (b), a code fix, not
   a snapshot update.
3. For `now_playing_snapshot_history`, read its `.snap.new` diff independently: if it is an
   intentional history-view change (e.g. from `06-05` listening history or `06-08` instant mix),
   accept that one snapshot only ((a)); if it looks like lost/misordered history content, fix
   `views/now_playing.rs` instead ((b)); if it differs only in a timestamp/clock value, inject a
   fixed `state.clock` in the test fixture and accept the snapshot ((c)).
4. Run `just check-all` until clean, and keep the final diff scoped to exactly the files named in
   the task (the fixed `.rs` file(s) and/or the accepted `.snap` file(s), nothing else).

No other files in this repository were changed in this pass.

### Added
- Workspace scaffolding: six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`,
  `loxia-cache`, `loxia-tui`, `loxia-player`) with the full module tree per `docs/01-architecture.md`.
- Locked dependency set per `docs/13-dependencies.md`.
