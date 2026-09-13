# 06-08 · Instant mix

**Phase:** 06 — Queue engine · **Agent:** A · **Size:** S
**Prerequisites:** `06-01`, `02-07`
**Reference:** `design_overview` §4.2

## Goal
`m` generates a radio-style queue seeded from whatever is selected.

## Files
- `crates/loxia-core/src/reducer/queue.rs` (extend)
- `crates/loxia-player/src/workers/network.rs` (extend)

## Specification

`Action::Queue(InstantMix)` takes the focused item's `ItemId` — it works for an artist, album,
track, genre, or playlist, since Emby's endpoint accepts any of them — and emits
`Effect::Net(InstantMix { seed, limit: 100 })`.

On `Data::TracksLoaded` for an instant-mix request:
1. **Replace** the queue rather than appending. A mix is a new listening session; appending 100
   tracks to an existing queue is never what the user meant.
2. Set `QueueSource::InstantMix { seed }` on every entry, so the queue view can badge them and the
   user can tell where they came from.
3. Start playback from index 0.
4. **Do not apply the sort profile.** The server's ordering is the recommendation; re-sorting it
   alphabetically would discard the entire point of the feature.
5. Toast `instant mix: <n> tracks from <seed name>`.

**Empty result** is not an error — some libraries genuinely cannot generate a mix. Leave the queue
untouched and toast `no instant mix available for <name>`. Clearing the user's queue and then
discovering there is nothing to replace it with would be actively harmful, so the replacement
happens **only** on a non-empty reply.

**Offline.** Refuse with `instant mix requires a connection`; the endpoint has no offline equivalent.

Multi-select seeds from the **first** selected item, and toasts that it did, so the behaviour is not
mysterious.

## Acceptance
- `instant_mix_emits_effect_with_selected_id`
- `instant_mix_works_for_every_item_type` — table test over artist, album, track, genre, playlist.
- `instant_mix_replaces_queue`
- `instant_mix_starts_playback_from_zero`
- `instant_mix_entries_carry_source_badge`
- `instant_mix_does_not_apply_sort_profile`
- `empty_instant_mix_leaves_queue_intact`
- `empty_instant_mix_toasts`
- `instant_mix_refused_when_offline`
- `multiselect_seeds_from_first_and_toasts`

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 06's exit criteria in
`docs/08-roadmap.md` are met.
