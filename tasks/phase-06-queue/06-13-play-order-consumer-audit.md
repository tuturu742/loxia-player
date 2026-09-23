# 06-13 — `play_order` consumer audit

## Prerequisites

- `06-12` — Stale-preload retraction

## Goal

**Audit only — no behaviour changes**, except for adding regression tests where the audit finds a
consumer that reads the queue's ordering field in a way that would silently desync from `06-10`'s
insert-next fix or `06-12`'s retraction. Enumerate every place in the workspace that reads the
queue's ordering (`play_order`, or whatever field/method `crates/loxia-core/src/state/queue.rs` and
`crates/loxia-core/src/queue.rs` actually name it — confirm the exact identifier as the first step
of this audit rather than assuming the name) to determine "what plays after this track", and
confirm each one is consistent with the authoritative order `06-10` established (shuffle order when
active, base order otherwise) and with `06-12`'s retraction/re-preload behaviour.

## Context

`06-10` fixed insert-next to always target the *authoritative* order. `06-12` made preload
retraction consult that same authoritative order. Neither task audited whether every other
consumer of that ordering already agreed, or was quietly relying on the base (unshuffled) order
directly. Likely candidates, to be confirmed rather than assumed:

- `crates/loxia-tui/src/views/now_playing.rs` — renders "up next" list.
- `crates/loxia-core/src/reducer/player.rs` — advancing to the next track on natural end.
- `crates/loxia-cache/src/scrobble.rs` and `crates/loxia-cache/src/session.rs` — anything that
  persists "what's next" across a restart (feeds `11-06` session restore).
- `crates/loxia-player/src/workers/mpris.rs` — if MPRIS exposes a "next track" concept to the OS.

## Files to touch

- New file: `docs/audits/06-13-play-order-consumers-findings.md`.
- Regression tests only, added to whichever of the files above the audit finds an actual
  inconsistency in — no non-test production code changes in this task. If a real inconsistency is
  found, add an `#[ignore]`d, `06-13`-cited test in the same style as `06-11`, and open a
  follow-up task file (numbered `06-14` or later, by whoever picks it up next) rather than fixing
  it inline — this task's scope is the audit, matching `06-11`'s precedent.

## Specification

The findings document must, per consumer file listed above (plus any other genuine consumer the
audit turns up):

1. Name the exact field/method it reads to determine ordering, quoting the real identifier.
2. State whether that read already goes through the same authoritative-order resolution `06-10`
   introduced, or bypasses it (reads the base order directly).
3. If it bypasses it: is that a bug (user-visible desync) or intentional (e.g. session persistence
   deliberately persisting the base order so a restored session doesn't re-shuffle differently) —
   state which, and why, citing the relevant doc or task file.
4. Cross-reference `06-12`'s retraction: does this consumer need to react to a retraction event at
   all, or is it purely presentational and self-corrects on the next render/tick?

## Acceptance

- `docs/audits/06-13-play-order-consumers-findings.md` exists and covers every file named in
  Context, at minimum, with a clear per-file verdict (consistent / bypasses-intentionally /
  bypasses-bug).
- Any bypass classified as a bug has a corresponding `#[ignore]`d regression test added next to the
  offending code, citing `06-13`, and a follow-up task is named (even if not yet created) in the
  findings document.
- No production code path changes as a result of this task.
