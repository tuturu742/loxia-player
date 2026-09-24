# Task handback: render::tests::layout_snapshot_200x50

## Verdict: cannot be determined from available information — handing back

I do not have tool access in this session to actually execute:

- `cargo test -p loxia-tui render::tests::layout_snapshot_200x50` (to capture the real
  `.snap.new` diff),
- `git log --follow -- crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap`
  (to find the commit that last changed the fixture),
- `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` (to find the
  commit that changed the rendering), or
- a second run under `TZ=UTC LANG=C` (to rule out nondeterminism per verdict (c)).

The repository context made available to me in this conversation shows most of
`crates/loxia-tui/src/render.rs`, all of `crates/loxia-tui/src/layout.rs`, and the full stored
`crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap` fixture as
either empty placeholders or truncated mid-content (the fixture itself is described as ~26KB but
only a small leading fragment was included). Without the complete, exact current text of:

1. the stored `.snap` file in full,
2. the actual `render.rs` / `layout.rs` source that produces the 200x50 buffer, and
3. the real `.snap.new` diff insta would produce today,

I cannot reliably classify the failure as (a) stale fixture, (b) rendering regression, or (c)
nondeterminism, per Step 1.4 of the task instructions. Guessing at either a full 26KB snapshot
body or a source-code fix without this evidence risks:

- committing a fixture that does not match what `cargo insta test --review` would actually
  generate (silently masking a real regression, or breaking the test again for a different
  reason), or
- editing `render.rs`/`layout.rs` in a way that incidentally changes the output for the sibling
  `layout_snapshot_80x24` and `layout_snapshot_120x30` tests, which this task explicitly forbids
  touching.

Per the task's own instructions ("If the root cause is in another crate... stop... hand the task
back") and the general principle in the project rules ("if a criterion cannot be tested as
written, say so rather than quietly reinterpreting it"), the safe action here is to hand this back
rather than fabricate a fix.

## What is needed to actually complete this task

Please supply (or grant tool access to produce):

- the complete, current contents of `crates/loxia-tui/src/render.rs` (specifically the
  `layout_snapshot_200x50` test and the `draw_at` helper it calls),
- the complete, current contents of `crates/loxia-tui/src/layout.rs` (the `zones()` function,
  since it determines column widths at 200x50),
- the complete, current contents of the stored
  `crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap` fixture,
  and
- the actual `.snap.new` diff from running
  `cargo test -p loxia-tui render::tests::layout_snapshot_200x50` (or
  `cargo insta test -p loxia-tui --review`) against the current `master`.

With those, the verdict step can be completed properly: diffing the stored fixture against the
`.snap.new`, checking `git log --follow` for the fixture's last edit, checking
`git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` for the commit
that changed rendering, and cross-checking any layout/text change against `docs/07-ui-spec.md`.

No source files or snapshot fixtures have been modified in this submission.
