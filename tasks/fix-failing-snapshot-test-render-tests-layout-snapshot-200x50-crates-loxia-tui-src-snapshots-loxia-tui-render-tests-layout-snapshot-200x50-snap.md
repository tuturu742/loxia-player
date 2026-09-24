# Fix failing snapshot test render::tests::layout_snapshot_200x50 (crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap)

- title: Fix failing snapshot test render::tests::layout_snapshot_200x50 (crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap)
- description: Fixture: `crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap`. This is the largest fixture, about 26KB.
Test: `render::tests::layout_snapshot_200x50` in `crates/loxia-tui/src/render.rs`
Rendering code: `crates/loxia-tui/src/render.rs`, which draws the full screen: the Miller columns, header, player bar and hint line.

Background: on a clean `master`, `cargo test --workspace` fails five insta snapshot tests in loxia-tui. Each one is its own task, and this task covers only the fixture above. The sibling fixtures are layout_snapshot_80x24, layout_snapshot_120x30, now_playing_snapshot_history and header_snapshot_offline_with_downloads. Do not edit their `.snap` files.

Step 1: decide before you change anything, and put the verdict at the top of the PR description.
1. Read the current stored `.snap` above in full, and read `render.rs` (the test and the code it calls).
2. Run `cargo test -p loxia-tui render::tests::layout_snapshot_200x50`. Capture the insta diff from the generated `.snap.new` or from `cargo insta test -p loxia-tui --review`. Do not accept it yet.
3. Run `git log --follow -- <fixture>` to find the commit that last changed the fixture. Then run `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` to find the commit that changed the rendering.
4. Classify the diff as one of:
   (a) Stale fixture: a commit or task under `tasks/` changed the layout deliberately, and the new output matches the design docs under `docs/`.
   (b) Rendering regression: the change was not intended. Examples are misaligned or wrongly sized columns, wrong padding or fill at wide widths, a lost indicator, text that contradicts `docs/`, or a keybinding hardcoded in UI text instead of rendered through `KeyMap::hint_for(ActionId)`.
   (c) Nondeterministic output: wall-clock time, relative timestamps, locale, hostname, env or terminal size leaking into the render. Run the test twice, and once with `TZ=UTC LANG=C` changed. If the output differs, the verdict is (c).
5. Write the verdict with the commit hash and one sentence of evidence. Do not assume it matches the other layout tests.

Step 2: act on the verdict.
- (a): rewrite this `.snap` file in full with the new output (`cargo insta accept` for this snapshot, or rename its `.snap.new`). Keep insta's header block (`---`, `source:`, `expression:`, `---`) in the same format, and keep trailing whitespace exactly as insta emits it. Check that the diff matches exactly the intentional change you cited.
- (b): fix the rendering in `render.rs` (or the loxia-tui code it calls) so the output matches the stored fixture again. Do not edit the `.snap`. Keep the fix minimal, and say in the PR whether it also affects the 80x24 and 120x30 tests.
- (c): make the test deterministic by injecting a fixed clock or fixture value in the test setup, then accept the resulting snapshot. Do not use insta redactions to hide real content.
- If the root cause is in another crate (for example a loxia-core model or keymap change), stop. Write up the cause in the PR and hand the task back. CONTRIBUTING.md forbids crossing crate boundaries without an authorising task.

Forbidden: deleting snapshots, setting `INSTA_FORCE_PASS` or `INSTA_UPDATE`, adding `#[ignore]`, weakening assertions, adding dependencies not listed in `docs/13-dependencies.md`, and `unwrap()`/`expect()` outside tests. Do not refactor, and do not touch tests that already pass.

Done when:
- `render::tests::layout_snapshot_200x50` passes.
- `git diff --stat` shows only this fixture and/or the loxia-tui source behind it.
- No stray `.snap.new` or `.pending-snap` files are committed.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
- The PR states the verdict ((a), (b) or (c)), the commit hash and the evidence, and says whether the snapshot or the code changed.

A reviewer checks the work by reading the cited commit next to the diff.

## Brief

The branch pyr/a9bba512-4 has no changes (the file list says 'Files changed (0)'). That list is never truncated, so nothing was touched, and render::tests::layout_snapshot_200x50 still fails exactly as it does on master. None of the 'Done when' criteria are met, and there is nothing to check against a cited commit. The branch must contain one of these outcomes of Step 1:

1. Verdict (a), stale fixture: commit the updated crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap.
   - Keep insta's header block (---, source:, expression:, ---) and its trailing whitespace exactly as insta emits them.
   - Do not commit any .snap.new or .pending-snap files.

2. Verdict (b), rendering regression: commit a minimal fix in crates/loxia-tui/src/render.rs, or in the loxia-tui code it calls, so the output matches the stored fixture.
   - Leave the .snap file untouched.
   - State in the PR whether the fix also affects layout_snapshot_80x24 and layout_snapshot_120x30.

3. Verdict (c), nondeterministic output: commit a fixed clock or fixture value in the test setup in render.rs, plus the accepted .snap for this fixture only.
   - Do not use redactions to hide real content.

In every case, put the verdict letter, the commit hash from `git log --follow` / `git log <commit>..master`, and one sentence of evidence at the top of the PR description.

If the root cause turned out to be in another crate (for example a loxia-core keymap or model change), an empty diff is the correct outcome. But that hand-back has to be written up explicitly in the PR description: the offending commit hash, the file in loxia-core, and why fixing it needs a cross-crate authorising task. Nothing in what I was given shows that write-up. If this is a hand-back, add it and I will accept the task as returned rather than done. Otherwise, push the fix.

Before resubmitting, confirm that `cargo test -p loxia-tui render::tests::layout_snapshot_200x50` passes, that `cargo fmt --all -- --ch

## Status

Scaffolded by the delegated coding agent. TODO: implement.
