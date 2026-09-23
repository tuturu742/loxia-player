# 06-10 · Insert-next consistency fix

**Phase:** 06 — Queue engine · **Agent:** core · **Size:** M · **Prerequisites:** 06-09 ·
**Reference:** docs/02-data-model.md, docs/04-state-and-input.md

## Goal

`06-09`'s characterization tests recorded the current, possibly-inconsistent behaviour of every
"insert next" call site. This task makes them actually consistent: inserting a track to play next
must always land immediately after the currently-playing track — in the order the player will
actually consume next, i.e. the shuffled order when shuffle is on — regardless of which call site
triggered the insert (a generic queue-insert action, appears-on insertion, or an instant-mix
batch), and regardless of how many "insert next" calls have already happened since the current
track started.

## Files

- `crates/loxia-core/src/reducer/queue.rs`
- `crates/loxia-core/src/queue.rs`
- `crates/loxia-core/src/queue/shuffle.rs`
- `crates/loxia-core/src/state/queue.rs`

## Specification

- Introduce (or consolidate onto) a single insert-next primitive that every call site funnels
  through, so there is exactly one place that decides "immediately after the current track" in
  terms of the order actually being played (post-shuffle, if shuffled).
- Repeated "insert next" calls must stack in call order: the most recently inserted track plays
  first, immediately after the current track, then the previously inserted one, then the original
  next track — i.e. each insert lands right after the current track, pushing earlier inserts (but
  not the rest of the queue) back by one.
- Update `06-09`'s tests to match the now-correct, now-consistent behaviour: any test whose
  `// characterization:` comment described a disagreement now describes agreement, and the
  comment is updated to `// regression:` (this is a fix being pinned down, not merely observed).

## Acceptance

- `insert_next_places_after_current_track_unshuffled` (updated if needed, still passing)
- `insert_next_position_when_shuffled` (updated if needed, still passing)
- `repeated_insert_next_relative_order` (updated if needed, still passing)
- `insert_next_agrees_across_call_sites` — must now assert agreement, not merely record
  disagreement
- `insert_next_is_position_independent_of_call_site` (new)

## Done when

See the Global Definition of Done in `tasks/README.md`, plus:
- all tests named above and in `06-09` exist and pass
- no `// characterization:` comment describing a disagreement remains in the files touched
