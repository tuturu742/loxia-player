# Pin insert-next and append queue behaviour with characterization tests

- title: Pin insert-next and append queue behaviour with characterization tests
- description: Scope: loxia-core only. Do not modify any other crate.

What is known:
- `QueueState` (`crates/loxia-core/src/state/queue.rs`) has `entries: Vec<QueueEntry>`, kept in insertion order and never reordered by shuffle.
- It also has `play_order: Vec<usize>`, a permutation of indices into `entries`, and `position: usize`, which indexes into `play_order`. The current entry is `entries[play_order[position]]`.
- `crates/loxia-core/src/queue/shuffle.rs` un-shuffles by setting `play_order = (0..entries.len()).collect()` and moving `position` to wherever the current entry now sits.
- 'Play next' (`i`) and 'add to queue' (`a`) already exist through `QueueBatch` / `QueueBatchMode` in `crates/loxia-core/src/reducer/queue.rs`. Nobody has read that code yet.
- `PlayerState.session`, `play_reported` and `start_reported` (`crates/loxia-core/src/state/player.rs`) are reset only by `reducer::queue::load_current` on a fresh `Load`.

Work: read the `QueueBatch` insert and append arms in `reducer/queue.rs`. Add tests next to the existing shuffle tests in the `#[cfg(test)]` module of `queue/shuffle.rs` (or in `reducer/queue.rs`'s tests, if the reducer entry point is only reachable there). Reuse the `queue_of` fixture. Required tests:
- `insert_next_unshuffled_plays_immediately_after_current`
- `insert_next_shuffled_plays_immediately_after_current`
- `insert_next_shuffled_then_unshuffle_keeps_entry_after_current`
- `insert_next_multi_select_preserves_selection_order`
- `append_unshuffled_goes_to_end`
- `append_shuffled_goes_to_end_of_play_order`
- `queue_edit_does_not_change_current_entry_or_position_target`
- `queue_edit_emits_no_load_and_keeps_session`: the effects list contains no `Effect::Audio(Load)`, and `session`, `play_reported` and `start_reported` are unchanged.
- `play_order_is_a_permutation_after_every_edit`

Keep the three existing shuffle tests unchanged.

Tests that fail against current behaviour must be marked `#[ignore = "fixed by <fix task id>"]` so CI stays green. List them in the PR description.

Done when: every named test exists. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` are clean. The PR description states which tests pass and which are ignored as known defects.

## Brief

<knowledge id="k1" class="lore" source="Why this project exists" entry="Business constraints">
Single maintainer: features that need staffing to operate are out of scope. Self-hosted first — no hosted service to sell, so the install must stay a one-liner. The engine is AGPL and the packs MIT, so anything that would force pack authors to open their content is a design error, not a licensing detail.
</knowledge>

<knowledge id="k2" class="lore" source="Why this project exists" entry="What users actually ask for">
In order of how often it comes up: 'the NPC blurted the twist' (the reason exclusion exists), 'the dice are made up', 'I can't tell why it said that', and 'I don't want my campaign on someone else's server'. Every one of those maps to a structural feature rather than a better prompt — that mapping is the product.
</knowledge>

<knowledge id="k3" class="lore" source="Why this project exists" entry="Who this is for">
Three audiences, in priority order. **Tabletop groups** who want a game master that can hold a secret and dice that cannot be talked out of a result. **Teams** running structured multi-agent working sessions where some facts are genuinely confidential. **Engineering orgs** delegating work to coding agents under review. The first pays the rent for the design; the other two prove the engine is domain-neutral.
</knowledge>

<knowledge id="k4" class="misc" source="Project reference shelf" entry="Decisions worth remembering">
Postgres-only was chosen over a vector database because the isolation guarantees live in RLS and a second store would need its own. CEL was chosen over any embedded scripting because user-authored code is a security stance we do not want to defend. Both decisions get re-proposed roughly twice a year; neither has changed.
</knowledge>

<knowledge id="k5" class="misc" source="Project reference shelf" entry="Team glossary">
**Overlay** — the per-workspace relabelling of core nouns. **Pack** — declarative workflow content (schemas, flo

## Status

Scaffolded by the delegated coding agent. TODO: implement.
