# Fix failing snapshot test views::now_playing::tests::now_playing_snapshot_history (crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap)

- title: Fix failing snapshot test views::now_playing::tests::now_playing_snapshot_history (crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap)
- description: Fixture: `crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap`
Test: `views::now_playing::tests::now_playing_snapshot_history` in `crates/loxia-tui/src/views/now_playing.rs`
Rendering code: `crates/loxia-tui/src/views/now_playing.rs`, the now-playing view with its play-history list.

Background: on a clean `master`, `cargo test --workspace` fails five insta snapshot tests in loxia-tui. Each one is its own task, and this task covers only the fixture above. The sibling fixtures are the three render layout snapshots and header_snapshot_offline_with_downloads. Do not edit their `.snap` files.

Step 1: decide before you change anything, and put the verdict at the top of the PR description.
1. Read the current stored `.snap` above in full, and read `now_playing.rs` (the test fixture setup and the history rendering code).
2. Run `cargo test -p loxia-tui views::now_playing::tests::now_playing_snapshot_history`. Capture the insta diff from the `.snap.new` or from `cargo insta test -p loxia-tui --review`. Do not accept it yet.
3. Run `git log --follow -- <fixture>` to find the commit that last changed the fixture. Then run `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` to find the commit that changed the rendering.
4. Classify the diff as one of:
   (a) Stale fixture: a commit or task under `tasks/` changed the history view deliberately, and the new output matches the design docs under `docs/`.
   (b) Rendering regression: the change was not intended. Examples are history in the wrong order (check the order `docs/` specifies), missing or duplicated entries, wrong fields or truncation, or a keybinding hardcoded instead of rendered through `KeyMap::hint_for(ActionId)`.
   (c) Nondeterministic output: history views often render relative times such as '3 min ago' or wall-clock times. Check whether the render reads the system clock, locale or timezone. Run the test twice, and once with `TZ=UTC LANG=C` changed. If the output differs, the verdict is (c).
5. Write the verdict with the commit hash and one sentence of evidence.

Step 2: act on the verdict.
- (a): rewrite only this `.snap` file with the new output (`cargo insta accept` for this snapshot, or rename its `.snap.new`). Keep insta's header block (`---`, `source:`, `expression:`, `---`) in the same format. Check that the diff matches exactly the intentional change you cited.
- (b): fix `now_playing.rs` so the output matches the stored fixture again. Do not edit the `.snap`, and add nothing beyond the fix.
- (c): make the test deterministic. Inject a fixed clock or fixed 'now' value into the test setup, and if the view has no seam for that, add a minimal one inside loxia-tui. Then accept the resulting snapshot. Do not use insta redactions to hide real content.
- If the root cause is in another crate (for example a loxia-core model or history-state change), stop. Write up the cause in the PR and hand the task back. CONTRIBUTING.md forbids crossing crate boundaries without an authorising task.

Forbidden: deleting snapshots, setting `INSTA_FORCE_PASS` or `INSTA_UPDATE`, adding `#[ignore]`, weakening assertions, adding dependencies not listed in `docs/13-dependencies.md`, and `unwrap()`/`expect()` outside tests. Do not refactor, and do not touch tests that already pass.

Done when:
- The test passes, and it passes the same way under two runs and under a changed `TZ`/`LANG`.
- `git diff --stat` shows only this fixture and/or `now_playing.rs` (or other loxia-tui source it calls).
- No stray `.snap.new` or `.pending-snap` files are committed.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
- The PR states the verdict ((a), (b) or (c)), the commit hash and the evidence, and says whether the snapshot or the code changed.

## Brief

The branch pyr/842abbf2-5 has no changes. The changed-file list is complete and shows 0 files, so truncation is not hiding anything. With nothing changed, `views::now_playing::tests::now_playing_snapshot_history` still fails as it does on master. None of the done criteria is met.

What to do:
1. Put the classification at the top of the PR description: (a) stale fixture, (b) rendering regression or (c) nondeterministic output. Include the commit hash from `git log --follow -- crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap` and from `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*`, plus one sentence of evidence.
2. Act on the verdict:
   - (a): regenerate only `crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap` and keep insta's `---`/`source:`/`expression:`/`---` header.
   - (b): fix `crates/loxia-tui/src/views/now_playing.rs` so it matches the stored fixture, without editing the `.snap`.
   - (c): add a fixed clock or fixed 'now' seam inside loxia-tui, use it in the test setup in `now_playing.rs`, then accept the snapshot. No redactions.
3. Do not touch the sibling fixtures (the three render layout snapshots and header_snapshot_offline_with_downloads). Commit no `.snap.new` or `.pending-snap` files.
4. Confirm in the PR that the test passes on two runs and under `TZ=UTC LANG=C`, and that `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.

If the root cause turns out to be in another crate, such as loxia-core history state, an empty diff is the correct outcome. In that case the PR description must say so explicitly, with the commit hash and evidence, and hand the task back. I can't see any such write-up, so as submitted this branch does not complete the work item.

## Status

Scaffolded by the delegated coding agent. TODO: implement.
