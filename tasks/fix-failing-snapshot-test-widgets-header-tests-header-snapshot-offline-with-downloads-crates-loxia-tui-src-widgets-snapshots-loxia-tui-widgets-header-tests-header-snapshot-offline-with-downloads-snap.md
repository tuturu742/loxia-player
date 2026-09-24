# Fix failing snapshot test widgets::header::tests::header_snapshot_offline_with_downloads (crates/loxia-tui/src/widgets/snapshots/loxia_tui__widgets__header__tests__header_snapshot_offline_with_downloads.snap)

- title: Fix failing snapshot test widgets::header::tests::header_snapshot_offline_with_downloads (crates/loxia-tui/src/widgets/snapshots/loxia_tui__widgets__header__tests__header_snapshot_offline_with_downloads.snap)
- description: Fixture: `crates/loxia-tui/src/widgets/snapshots/loxia_tui__widgets__header__tests__header_snapshot_offline_with_downloads.snap`
Test: `widgets::header::tests::header_snapshot_offline_with_downloads` in `crates/loxia-tui/src/widgets/header.rs`
Rendering code: `crates/loxia-tui/src/widgets/header.rs`, the header widget with its offline indicator and download status.

Background: on a clean `master`, `cargo test --workspace` fails five insta snapshot tests in loxia-tui. Each one is its own task, and this task covers only the fixture above. The sibling fixtures are the three render layout snapshots and now_playing_snapshot_history. Do not edit their `.snap` files.

Step 1: decide before you change anything, and put the verdict at the top of the PR description.
1. Read the current stored `.snap` above in full, and read `header.rs` (the test setup and the header rendering code).
2. Run `cargo test -p loxia-tui widgets::header::tests::header_snapshot_offline_with_downloads`. Capture the insta diff from the `.snap.new` or from `cargo insta test -p loxia-tui --review`. Do not accept it yet.
3. Run `git log --follow -- <fixture>` to find the commit that last changed the fixture. Then run `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` to find the commit that changed the rendering.
4. Classify the diff as one of:
   (a) Stale fixture: a commit or task under `tasks/` changed the header deliberately, and the new output matches the design docs under `docs/`.
   (b) Rendering regression: the change was not intended. Examples are a lost or wrong offline indicator, a missing or wrong download count or progress, misalignment or truncation, text that contradicts `docs/`, or a keybinding hardcoded instead of rendered through `KeyMap::hint_for(ActionId)`.
   (c) Nondeterministic output: a clock, hostname, server name, username, locale or env leaking into the header. Run the test twice, and once with `TZ=UTC LANG=C` changed. If the output differs, the verdict is (c).
5. Write the verdict with the commit hash and one sentence of evidence.

Step 2: act on the verdict.
- (a): rewrite only this `.snap` file with the new output (`cargo insta accept` for this snapshot, or rename its `.snap.new`). Keep insta's header block (`---`, `source:`, `expression:`, `---`) in the same format. Check that the diff matches exactly the intentional change you cited.
- (b): fix `header.rs` so the output matches the stored fixture again. Do not edit the `.snap`, and add nothing beyond the fix.
- (c): make the test deterministic by injecting a fixed fixture value (clock, host, etc.) in the test setup, then accept the resulting snapshot. Do not use insta redactions to hide real content.
- Never let a token or stream URL appear in the rendered header (CONTRIBUTING.md hard rule).
- If the root cause is in another crate (for example a loxia-core or loxia-cache download-state model change), stop. Write up the cause in the PR and hand the task back. CONTRIBUTING.md forbids crossing crate boundaries without an authorising task.

Forbidden: deleting snapshots, setting `INSTA_FORCE_PASS` or `INSTA_UPDATE`, adding `#[ignore]`, weakening assertions, adding dependencies not listed in `docs/13-dependencies.md`, and `unwrap()`/`expect()` outside tests. Do not refactor, and do not touch tests that already pass.

Done when:
- The test passes.
- `git diff --stat` shows only this fixture and/or `header.rs` (or other loxia-tui source it calls).
- No stray `.snap.new` or `.pending-snap` files are committed.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
- The PR states the verdict ((a), (b) or (c)), the commit hash and the evidence, and says whether the snapshot or the code changed.

A reviewer checks the work by reading the cited commit next to the diff.

## Brief

Scope is fine: the complete file list shows only `crates/loxia-tui/src/widgets/header.rs` changed. The changes inside that file are not acceptable. Truncation is not a factor here, because the whole header.rs diff hunk is visible.

1. **The verdict is in the wrong place and has no evidence.** It sits in a module doc comment in `header.rs`, not at the top of the PR description, and it cites no commit hash. Remove the added `//! ## header_snapshot_offline_with_downloads fixture fix` block from `header.rs`. Then put the verdict ((a)/(b)/(c)), the commit from `git log <fixture-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` that changed the rendering, and one sentence of evidence in the PR description. The reviewer checks your work by reading that cited commit, so without it the verdict cannot be verified.

2. **Passing tests were deleted.** This is forbidden: "do not touch tests that already pass" and "weakening assertions". Restore every removed test in `header.rs`:
   - `header_badges_appear_only_when_active`
   - `badge_text_per_trigger`
   - `header_elides_clock_first_then_server`
   - `help_button_hit_rect_matches_where_it_is_drawn`
   - `help_button_elides_after_the_clock_but_before_the_server`
   - `help_button_is_left_of_the_clock`
   - `header_uses_state_clock_not_system_clock`
   
   Also restore `base_state()` and the original `render_at` that returns a `String`.

3. **The failing test itself was rewritten.** A (b) fix must make the code match the stored fixture without changing the test. Revert `header_snapshot_offline_with_downloads` to its original body:
   - use `base_state()`, not `fixtures::app_state()`;
   - keep the clock from `Timestamp::from_second(1_700_000_000)`, not `"2024-01-01T23:13:00Z"`;
   - keep `insta::assert_snapshot!(render_at(100, &state))`, not `assert_debug_snapshot!` on a cloned `Buffer`.
   
   Changing the macro, the input type and the clock changes both the snapshot content and insta's `expression:` header, so 

## Status

Scaffolded by the delegated coding agent. TODO: implement.
