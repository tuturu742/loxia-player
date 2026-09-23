# 06-13 · Play-order consumer audit

**Phase:** 06 — Queue engine · **Agent:** core · **Size:** S · **Prerequisites:** 06-12 ·
**Reference:** docs/02-data-model.md, docs/04-state-and-input.md

## Goal

`06-10` made queue-insert semantics consistent and `06-12` made preloads track that consistent
order instead of going stale. This task closes the loop: audit every remaining reader of the
queue's play order (not just the audio-preload path `06-11`/`06-12` covered) to confirm none of
them still assumes the old, pre-`06-10` insert behaviour — e.g. an "up next" list in `loxia-tui`,
listening-history recording, or playback-reporting order — and record the audit so a future task
doesn't have to redo this investigation from scratch.

## Files

- `docs/audits/06-13-play-order-consumer-audit.md` (new)
- `#[cfg(test)]` additions only, alongside any consumer this audit finds actually depended on the
  old behaviour (may touch test modules in `loxia-core` and/or `loxia-tui`, wherever such a
  consumer is found) — no non-test change unless a consumer is found to be genuinely broken, in
  which case treat that as a separate follow-up task rather than expanding this one

## Specification

- Grep the workspace for every reader of the queue's ordering field (however `06-01` named it —
  e.g. `play_order`, the shuffled index list, or equivalent) outside `reducer/queue.rs` and
  `queue/shuffle.rs` themselves.
- For each consumer found, state in the audit file: what it reads, whether it could have observed
  the pre-`06-10` inconsistency, and whether it still behaves correctly now that `06-10`/`06-12`
  are in place.
- Where a consumer is found to still assume old behaviour, add a named regression test
  demonstrating the current (now presumably correct) behaviour at that consumer, so the finding is
  pinned down rather than left as prose.

## Acceptance

- `docs/audits/06-13-play-order-consumer-audit.md` exists and lists a verdict for every consumer
  found.
- Any consumer the audit calls out as previously assuming old behaviour has a named regression
  test (named in the audit entry itself) that exists and passes.

## Done when

See the Global Definition of Done in `tasks/README.md`, plus:
- the audit file exists with a verdict per consumer
- every regression test the audit file names exists and passes
