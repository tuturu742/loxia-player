# Fix failing snapshot test render::tests::layout_snapshot_120x30 (crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_120x30.snap)

- title: Fix failing snapshot test render::tests::layout_snapshot_120x30 (crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_120x30.snap)
- description: Fixture: `crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_120x30.snap`
Test: `render::tests::layout_snapshot_120x30` in `crates/loxia-tui/src/render.rs`
Rendering code: `crates/loxia-tui/src/render.rs`, which draws the full screen: the Miller columns, header, player bar and hint line.

Background: on a clean `master`, `cargo test --workspace` fails five insta snapshot tests in loxia-tui. Each one is its own task, and this task covers only the fixture above. The sibling fixtures are layout_snapshot_80x24, layout_snapshot_200x50, now_playing_snapshot_history and header_snapshot_offline_with_downloads. Do not edit their `.snap` files.

Step 1: decide before you change anything, and put the verdict at the top of the PR description.
1. Read the current stored `.snap` above in full, and read `render.rs` (the test and the code it calls).
2. Run `cargo test -p loxia-tui render::tests::layout_snapshot_120x30`. Capture the insta diff from the generated `.snap.new` or from `cargo insta test -p loxia-tui --review`. Do not accept it yet.
3. Run `git log --follow -- <fixture>` to find the commit that last changed the fixture. Then run `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` to find the commit that changed the rendering.
4. Classify the diff as one of:
   (a) Stale fixture: a commit or task under `tasks/` changed the layout deliberately, and the new output matches the design docs under `docs/`.
   (b) Rendering regression: the change was not intended. Examples are a truncated or misaligned column, a lost indicator, text that contradicts `docs/`, or a keybinding hardcoded in UI text instead of rendered through `KeyMap::hint_for(ActionId)`.
   (c) Nondeterministic output: wall-clock time, relative timestamps, locale, hostname, env or terminal size leaking into the render. Run the test twice, and once with `TZ=UTC LANG=C` changed. If the output differs, the verdict is (c).
5. Write the verdict with the commit hash and one sentence of evidence. The three layout tests share `render.rs`, but do not assume they share a verdict.

Step 2: act on the verdict.
- (a): rewrite only this `.snap` file with the new output (`cargo insta accept` for this snapshot, or rename its `.snap.new`). Keep insta's header block (`---`, `source:`, `expression:`, `---`) in the same format. Check that the diff matches exactly the intentional change you cited.
- (b): fix the rendering in `render.rs` (or the loxia-tui code it calls) so the output matches the stored fixture again. Do not edit the `.snap`. Keep the fix minimal, and say in the PR whether it also affects the 80x24 and 200x50 tests.
- (c): make the test deterministic by injecting a fixed clock or fixture value in the test setup, then accept the resulting snapshot. Do not use insta redactions to hide real content.
- If the root cause is in another crate (for example a loxia-core model or keymap change), stop. Write up the cause in the PR and hand the task back. CONTRIBUTING.md forbids crossing crate boundaries without an authorising task.

Forbidden: deleting snapshots, setting `INSTA_FORCE_PASS` or `INSTA_UPDATE`, adding `#[ignore]`, weakening assertions, adding dependencies not listed in `docs/13-dependencies.md`, and `unwrap()`/`expect()` outside tests. Do not refactor, and do not touch tests that already pass.

Done when:
- `render::tests::layout_snapshot_120x30` passes.
- `git diff --stat` shows only this fixture and/or the loxia-tui source behind it.
- No stray `.snap.new` or `.pending-snap` files are committed.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
- The PR states the verdict ((a), (b) or (c)), the commit hash and the evidence, and says whether the snapshot or the code changed.

A reviewer checks the work by reading the cited commit next to the diff.

## Brief

The branch pyr/842abbf2-3 has no changes: the complete file list reads 'Files changed (0)'. Truncation is not a factor here, because the list is never truncated and it is empty. As submitted, nothing makes render::tests::layout_snapshot_120x30 pass, so the work item is unmet.

Push one of the following, depending on your Step 1 verdict.

(a) Stale fixture: update only crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_120x30.snap with the accepted output.
- Keep insta's header block (---, source:, expression:, ---) in the same format.
- In the PR, cite the commit or tasks/ entry that changed the layout on purpose.

(b) Rendering regression: make a minimal fix in crates/loxia-tui/src/render.rs (or the loxia-tui code it calls) so the output matches the stored fixture again.
- Leave the .snap untouched.
- Say whether the fix also affects layout_snapshot_80x24 and layout_snapshot_200x50.

(c) Nondeterminism: inject a fixed clock or fixture value in the test setup in render.rs, then accept the resulting snapshot for this test only.
- Do not use insta redactions to hide real content.

If the root cause is in another crate (for example a loxia-core model or keymap change), an empty diff is the correct outcome under CONTRIBUTING.md. In that case the PR description must carry the full hand-back write-up:
- the verdict,
- the commit hash found via git log <fixture-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*,
- one sentence of evidence naming the cross-crate cause.
I cannot see a PR description in this review. If that write-up exists, re-submit with it attached and I will treat it as a hand-back, not a failed fix.

For any of (a), (b) or (c):
- The PR description must open with the verdict, the commit hash and the evidence, and say whether the snapshot or the code changed.
- git diff --stat must show only this fixture and/or loxia-tui source.
- No .snap.new or .pending-snap files may be committed.
- cargo fmt --all -- --check and car

## Status

Scaffolded by the delegated coding agent. TODO: implement.
