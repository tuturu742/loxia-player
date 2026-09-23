# 06-11 — Stale-preload audit

## Prerequisites

- `06-06` — Gapless preloading
- `06-07` — Playback reporting wiring
- `06-10` — Insert-next consistency fix

## Goal

**Audit only — no production code changes.** Enumerate every queue mutation that can happen after
`AudioCommand::Preload` has already been sent for "the next track" (per `06-06`,
`crates/loxia-audio/src/gapless.rs`: "`reducer::queue` ... is what decides *when* to send that
`Preload`"), such that mpv's own internal playlist is left holding a preloaded file that is no
longer actually next in `QueueState`. Deliverable is a written findings document plus a small set
of new, currently-failing (`#[ignore]`d, each citing this task by ID) regression tests in
`loxia-core` that pin down each stale-preload scenario found. `06-12` is the fix; this task is
strictly the audit.

## Context

Gapless preloading (`06-06`) appends the next track to mpv's own playlist ahead of time. Nothing in
`06-06`'s own scope retracts that append if the queue changes underneath it before the current
track finishes — e.g. the user does "play next" (`06-09`/`06-10`), removes the track that was
preloaded, reshuffles, or the sort profile changes and reorders the tail of the queue. `06-06`'s own
module doc says explicitly that *when* to preload is `reducer::queue`'s job — this task is about
finding every place that decision, once made, is invalidated without anything telling mpv.

## Files to touch

- New file: `docs/audits/06-11-stale-preload-findings.md` — the findings document (create the
  `docs/audits/` directory if it does not exist).
- `crates/loxia-core/src/reducer/queue.rs` — new `#[ignore]`d regression tests only, one per
  scenario found, each with a doc-comment citing `06-11` and a one-line description of the stale
  state it captures.
- `crates/loxia-audio/src/gapless.rs` — module doc: append a short "known gap, see `06-11`" note if
  the audit's findings live partly at this boundary (e.g. `AudioCommand` has no way to retract a
  preload at all yet — that absence is itself a finding, not something to fix here).

## Specification

The findings document must, at minimum, answer:

1. Which reducer actions (in `crates/loxia-core/src/action.rs`) can change what track immediately
   follows the currently-playing one, after a `Preload` has already been issued for it? (Expected
   candidates: remove-from-queue, play-next/insert-next per `06-10`, reshuffle/shuffle-toggle per
   `06-03`, sort-profile change per `06-04`.)
2. For each such action, does the current reducer emit any `Effect` that would cause the audio
   worker (`crates/loxia-player/src/workers/audio.rs`) to tell `loxia-audio` to drop or replace its
   outstanding preload? (Expected answer, to be confirmed by the audit, not assumed: no —
   `AudioCommand` per `crates/loxia-audio/src/backend.rs` has `Preload` but no corresponding
   retract/cancel variant.)
3. What is the user-visible symptom in each case — the wrong track plays gaplessly after the
   current one ends, a duplicate `Preload` stacks in mpv's playlist, or something else? State this
   per scenario, not generically.
4. Which crate boundary each fix will need to cross to close the gap (this directly informs
   `06-12`'s scope).

## Acceptance

- `docs/audits/06-11-stale-preload-findings.md` exists, is non-empty, and answers all four
  questions above with specific file/line references.
- At least one `#[ignore]`d regression test per confirmed scenario exists in
  `crates/loxia-core/src/reducer/queue.rs`, compiles, and is *expected* to fail once un-ignored
  (i.e. it currently demonstrates the stale state, not the fixed one).
- No file outside `docs/audits/` and the `#[cfg(test)]` modules named above is modified.
