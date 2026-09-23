# 06-11 · Stale-preload audit

**Phase:** 06 — Queue engine · **Agent:** `loxia-core` · **Size:** S · **Prerequisites:** `06-06`, `06-10` · **Reference:** `docs/05-audio-engine.md` §2, `docs/12-decisions.md` §9

## Goal

Determine, with tests and a written inventory (not a fix), whether an already-issued
`AudioCommand::Preload` can go stale — still pointing at a track that a subsequent queue mutation
(insert-next, remove, shuffle toggle, sort change) has displaced from the "plays next" position —
and name every place in the codebase, including outside `loxia-core`, that this can affect.

This task makes **no behaviour change**. It is scoped entirely to `loxia-core` for its own tests
and documentation; it does not need `06-12`'s cross-crate authorisation, because it only *reads*
and *documents* the code in `loxia-audio`/`loxia-player`, it does not modify it.

## Files

- `crates/loxia-core/src/reducer/queue.rs` (tests + an `// AUDIT` doc comment; no behaviour
  change)

## Specification

- Add an `// AUDIT` doc comment above `reducer/queue.rs`'s preload-issuing function, inventorying
  every queue mutation that can invalidate an in-flight preload, and stating for each whether the
  current code re-issues, retracts, or leaves stale the preload it already sent. Expect at least
  one "leaves stale" finding — that finding is what `06-12` fixes.
- Add characterization tests proving the queue reducer does **not** currently emit a corrected
  `Effect::Audio(AudioCommand::Preload(..))` (or any retraction effect) when the "next" track
  changes after a preload was already issued for the previous "next" track, for each mutation
  named in the doc comment.
- The `// AUDIT` doc comment must also name, by file path and function name, every call site
  outside `loxia-core` that receives a `Preload` command or a preload-related event — at minimum
  `crates/loxia-audio/src/mpv/handle.rs` (or `gapless.rs`, whichever actually implements it) and
  `crates/loxia-player/src/workers/audio.rs`. This inventory is what `06-12` uses to scope its own
  crate-crossing change; do not modify those files here.

## Acceptance

- `preload_not_reissued_after_insert_next_changes_the_next_track`
- `preload_not_reissued_after_remove_changes_the_next_track`
- `preload_not_reissued_after_shuffle_toggle_changes_the_next_track`
- `audit_comment_lists_every_cross_crate_preload_consumer` — a test asserting the `// AUDIT`
  inventory (expressed as a doc comment or an adjacent constant list, whichever this task's
  implementation chooses) names both `loxia-audio` and `loxia-player` file paths

## Done when

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] Every named test in the Acceptance section exists and passes
- [ ] No `docs/12-decisions.md` row added by this task — it is an audit, not a behaviour change;
      `06-12` records the fix
- [ ] Public items documented; the crate's `lib.rs` module list updated
- [ ] No dependency added that is not in `docs/13-dependencies.md`
- [ ] This task's checkbox ticked in `tasks/README.md`
