# Confirm or refute the stale gapless preload after a queue edit

- title: Confirm or refute the stale gapless preload after a queue edit
- description: The hypothesis, not yet verified:
- `PlayerState.last_preloaded: Option<QueueEntryId>` (`crates/loxia-core/src/state/player.rs`) lets `reducer::queue::preload_effects` avoid re-appending a file to mpv's playlist.
- Nothing documented removes a file that was already preloaded and has stopped being the next target.
- The `AudioCommand` enum reportedly has `Preload` but no remove or retract variant.
- So if the user presses `i` after a preload was sent, mpv may play the stale file gaplessly, and the queue and mpv would then disagree.

Read, and cite file and symbol names in the write-up:
- `crates/loxia-core/src/reducer/queue.rs`: `preload_effects`, `load_current`, and the handler for the audio 'track ended / playlist advanced' event. Does advance take `play_order[position + 1]` from queue state, or trust whatever mpv moved to?
- `crates/loxia-audio/src/backend.rs`: the full `AudioCommand` enum.
- `crates/loxia-audio/src/gapless.rs`: what happens when a new `Preload` arrives while one is already pending. Does it replace, append or ignore?
- The audio worker in `crates/loxia-player`, to see how `Preload` is translated into mpv playlist commands.

Add one loxia-core reducer test, `insert_next_after_preload_retargets_next_track`. Set up a queue whose next target is already recorded in `last_preloaded`, run insert-next, and assert on the emitted effects. Then simulate the track-ended event and assert that the new entry becomes current. If this exposes the bug, mark the test `#[ignore = "fixed by <retraction task id>"]`.

No production code changes in this task.

Done when: there is a short written finding saying the stale-preload bug is confirmed or refuted, with quoted code. The test exists. The retraction task's file is updated with the confirmed mechanism, or marked not needed.

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
