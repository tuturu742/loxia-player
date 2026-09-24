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

Scope is fine: only `crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_120x30.snap` changed, and no sibling `.snap` files or `.snap.new` files were touched. The problem is that the accepted output does not look like a deliberate layout change. It looks like a rendering regression (b) baked into the fixture.

What the hunk shows. It covers the whole style list through its closing `]` and `}`, so this is the complete change to the file, not a truncated view.
1. Lines 1–45 are unchanged. That includes the header and, by position, the buffer text content. So the rendered characters are the same as before.
2. The style list is cut from 179 lines to 23. Every style entry for rows y=5 to y=27 is gone:
   - the column separators at x=16, 61 and 119 on each row;
   - the dimmed Indexed(8) third column;
   - the dimmed spans at y=12 and y=13 (x=18..60, 35..43, 84..99);
   - the player bar row at y=25;
   - the hint line styling at y=27 (x=1 Indexed(8)).
3. The remaining rows 1–4 also change. The second divider moves from x=61/62 to x=63/64, and the inactive third column's Indexed(8) becomes Indexed(12).

Why this points to a regression. Identical text with most styling stripped below row 4 is not what an intentional layout change produces. It matches the brief's (b) examples: a lost indicator, a misaligned column, and hint and player-bar styling that no longer matches `docs/`. Case (c) is also possible if the terminal size or env is leaking in. In neither case is accepting the output correct.

What to change:
1. Revert `crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_120x30.snap` to master (8fe3ad8).
2. Rerun steps 1.2–1.4. Run the test twice, and once with `TZ=UTC LANG=C`. Inspect `render.rs` for why the column/divider styling and the dim Indexed(8) styling stop after row 4 at 120x30, and why the divider shifts by 2 columns.
3. If the cause is (b), fix it in `crates/loxia-tui/src/render.rs` (or the loxia-tui code it calls) so t

## Status

Scaffolded by the delegated coding agent. TODO: implement.
