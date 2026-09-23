# Hand-back: render::tests::layout_snapshot_200x50

## Why this is a hand-back, not a fix

I do not have working `cargo`/`git`/`insta` tool access in this session (no shell execution
capability), and the repository content provided to me for `crates/loxia-tui/src/render.rs` and
for `crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap` is
**truncated** — I only have partial excerpts of both files, not their complete contents. I cannot
safely rewrite either file in full without the risk of:

- corrupting `render.rs` (breaking compilation for the whole `loxia-tui` crate, and therefore the
  sibling `layout_snapshot_80x24` and `layout_snapshot_120x30` tests I am required not to touch),
  or
- hand-constructing a `.snap` body that does not actually match what `Buffer` would emit, which
  would leave the test failing in a different way.

I am therefore unable to complete Step 1.2–1.4 (capture the real insta diff, run
`git log --follow -- <fixture>`, run `git log <commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*`)
as instructed, because those require actual command execution against the real repository state,
which is not available to me here.

## Evidence pointing at another crate

The one part of the fixture I *can* see (the stored `.snap`, partially) contains this literal
text on the "Now Playing" row:

```
"▸ ♪ Now Playing ┌PLAY QUEUE  [unbound] History───────────────────────────────────────────────────┐┌...
```

`[unbound]` is the literal fallback string `KeyMap::hint_for(ActionId)` renders when the given
`ActionId` has **no bound chord** — see `CONTRIBUTING.md`'s hard rule: *"Never hardcode a
keybinding in UI text — render through `KeyMap::hint_for(ActionId)`."* The now-playing section
header is calling `hint_for` correctly (it is not a literal string baked into `loxia-tui`), and
the fact that it now resolves to "no binding" rather than a real chord is consistent with a
default keymap entry disappearing or being renamed in `loxia-core::keymap::defaults` (or a
default no longer surviving `loxia-core::keymap::validate`), i.e. a change in `loxia-core`, not in
`loxia-tui::render`.

Per this task's own instructions:

> If the root cause is in another crate (for example a loxia-core model or keymap change), stop.
> Write up the cause in the PR and hand the task back. CONTRIBUTING.md forbids crossing crate
> boundaries without an authorising task.

## What I could not produce, and why

- **Commit hash / `git log` evidence**: I have no shell access in this session to run
  `git log --follow -- crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap`
  or `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*`, so I
  cannot cite the exact commit hash the reviewer needs. This is a limitation of the tooling
  available to me, not a decision to skip that step.
- **Insta diff**: likewise, I cannot run `cargo test -p loxia-tui render::tests::layout_snapshot_200x50`
  or `cargo insta test -p loxia-tui --review` to capture the actual `.snap.new` diff.

## Formal hand-back

Per `CONTRIBUTING.md`'s crate-boundary rule, I am handing this task back:

- **Crate implicated**: `loxia-core`
- **File implicated**: `crates/loxia-core/src/keymap/defaults.rs` (and/or
  `crates/loxia-core/src/keymap/resolve.rs` / `validate.rs`), which supplies the binding that
  `KeyMap::hint_for(ActionId)` renders on the "Now Playing" queue section header.
- **Commit hash**: not available — I could not execute `git log` in this session to identify it.
  Whoever runs this task next should run:
  ```
  git log --follow -- crates/loxia-tui/src/snapshots/loxia_tui__render__tests__layout_snapshot_200x50.snap
  git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*
  ```
  and confirm whether the second command shows a `loxia-core/src/keymap/defaults.rs` change that
  removed or renamed the action bound to the "Now Playing" queue view's history/queue toggle
  action.
- **Evidence**: the stored fixture itself renders the literal fallback string `[unbound]` in the
  queue header, which is `KeyMap::hint_for`'s own "no chord bound" output, not a hardcoded
  `loxia-tui` string — implicating an upstream `loxia-core` keymap change rather than
  `crates/loxia-tui/src/render.rs`.

No source files or `.snap` fixtures are changed in this submission. I did not touch
`crates/loxia-tui/src/render.rs`, any `.snap` file (including the target
`loxia_tui__render__tests__layout_snapshot_200x50.snap`), or any sibling fixture, per the
instruction not to guess at a fix I cannot verify.
