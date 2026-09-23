# 06-11 · Stale preload audit

**Phase:** 06 — Queue engine · **Agent:** audio · **Size:** S · **Prerequisites:** 06-06, 06-10 ·
**Reference:** docs/05-audio-engine.md, crates/loxia-audio/src/gapless.rs

## Goal

`06-06` preloads the next track into mpv's playlist ahead of time (`AudioCommand::Preload`), so
playback is gapless. But a queue mutation between the preload and the actual track change —
`06-10`'s now-consistent "insert next", a removal, or a reshuffle — can make that already-issued
preload wrong: mpv would still play the *old* next track back to back with no gap, silently
ignoring the mutation the user just made. This task only finds and documents every place that can
happen; it fixes nothing. `06-12` fixes what this task finds.

## Files

- `docs/audits/06-11-stale-preload-audit.md` (new — the audit report)
- `crates/loxia-core/src/reducer/queue.rs` (`#[cfg(test)]` module only — reproduction tests)
- `crates/loxia-audio/src/gapless.rs` (module doc-comment only — append one sentence pointing at
  the audit report; no other change)

## Specification

Enumerate every place `AudioCommand::Preload` is sent (`grep -rn "Preload" crates/`) and, for
each, determine:

1. What queue mutations can occur between that preload being sent and the current track's natural
   end.
2. For each such mutation, whether the preloaded track still matches what the queue now says is
   next.
3. Whether anything today retracts or replaces a stale preload (expected finding: no).

Record the answer for every call site in `docs/audits/06-11-stale-preload-audit.md`, and add
`#[cfg(test)]` reproduction tests in `reducer/queue.rs` that demonstrate staleness where the audit
finds it — these may assert the current, buggy outcome (the preload target disagreeing with the
queue's new next track), with a `// characterization:` comment, exactly as in `06-09`.

## Acceptance

- `docs/audits/06-11-stale-preload-audit.md` exists and lists a verdict for every `Preload` call
  site found by the grep above.
- `preload_goes_stale_after_insert_next`
- `preload_goes_stale_after_removal`

No file outside `docs/audits/`, the `#[cfg(test)]` modules named above, and the single
module-doc sentence appended to `crates/loxia-audio/src/gapless.rs` is modified.

## Done when

See the Global Definition of Done in `tasks/README.md`, plus:
- the audit file and both named tests above exist
- the only non-test, non-doc change in the diff is the one-sentence addition to
  `crates/loxia-audio/src/gapless.rs`'s module doc comment
