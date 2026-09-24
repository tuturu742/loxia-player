# Make the loxia-tui snapshot suite green in one branch

- title: Make the loxia-tui snapshot suite green in one branch
- description: `cargo test --workspace --lib --bins --tests` fails four snapshot tests on this branch.
It failed five; `.cargo/config.toml` already pins TZ=UTC and one fixture is fixed, so the
cause is settled and what remains is mechanical.

READ THIS BEFORE EDITING ANYTHING.

You emit whole files. A snapshot fixture here is up to 38,000 bytes of box-drawing
characters, and the last attempt tried to rewrite four of them in one answer and dropped
lines from every one -- the 200x50 fixture came back 3,943 bytes shorter than it started.
A fixture that is short by one line fails exactly like a fixture that is wrong.

So: FIX EXACTLY ONE SNAPSHOT FILE PER ROUND.

1. Pick the single failing fixture named in the build output.
2. Copy its current contents from the repository verbatim. Change ONLY the lines the
   build output shows differing -- these are clock digits, HH:MM. Change nothing else:
   not a box character, not a space, not a trailing line.
3. Emit that ONE .snap file, and CHANGELOG.md only if it needs a line. Do not emit any
   other .snap file. Re-emitting a fixture that already passes is how a fixed test goes
   red again.
4. Say in the PR description which fixture you changed and which clock values you changed
   it to.

Done when: `cargo test --workspace --lib --bins --tests` passes with 0 failures, no
.snap.new or .pending-snap files are committed, and every .snap file is byte-identical to
its original except for the clock digits.


## Brief

The build FAILED, and this work item's acceptance condition is that `cargo test --workspace --lib --bins --tests` passes with 0 failures. That condition is not met, so I can't approve, even though this round's change is scoped correctly.

What this round got right:
- The complete file list is `.cargo/config.toml`, `CHANGELOG.md`, and one `.snap` file: `header_snapshot_offline_with_downloads.snap`. That follows the one-fixture-per-round rule.
- The visible hunk changes only the clock digits (`23:13` -> `22:13`) and leaves box characters and spacing alone.
- No `.snap.new` or `.pending-snap` files are in the list.
- `.cargo/config.toml` appears as Added relative to base. The work item says the TZ pin already exists on this branch, so it is consistent with earlier rounds and not new scope.

What limits this review:
- The build log I was given is cut off inside the crate-download phase. It never reaches compilation or test output.
- So I have no failing-test names, and no diffs, from this build. I cannot confirm from the log whether the header fixture now passes.

Failing tests, as far as I can tell:
- I was not given any failing test names in this build output.
- From the work item and the CHANGELOG, the fixtures still expected to fail are the ones this branch has not touched:
  - `render::tests::layout_snapshot_80x24`
  - `render::tests::layout_snapshot_120x30`
  - `render::tests::layout_snapshot_200x50`
  - `views::now_playing::tests::now_playing_snapshot_history`
- All four are this item's own targets, not pre-existing or out of scope. They are the exact failures the item exists to fix.
- If the full log shows `widgets::header::tests::header_snapshot_offline_with_downloads` still failing, the `22:13` value in that `.snap` is wrong. Correct it to whatever HH:MM the test's reported output shows.

Next round:
1. Run the build and get the full test output.
2. Pick exactly ONE of the still-failing fixtures. I suggest the smallest first: `crates/loxia-tui/src/snapshots/*r

## Status

Scaffolded by the delegated coding agent. TODO: implement.
