# 07-02 · Favourites tab

**Phase:** 07 — Views · **Agent:** D · **Size:** S
**Prerequisites:** `07-01`
**Reference:** `docs/07-ui-spec.md` §9

## Goal
Browse favourited artists, albums, and tracks, with optimistic unfavouriting that rolls back on
failure.

## Files
- `crates/loxia-tui/src/views/favourites.rs`
- `crates/loxia-core/src/reducer/queue.rs` (extend for `ToggleFavorite`)

## Specification

**Layout.** The same three-section layout as Search (task `07-01`) — reuse that widget rather than
duplicating it; extract it to `widgets/sectioned_list.rs` in this task if it is not already
shared. Data comes from `Effect::Net(FetchColumn { Favourites })`, loaded on first tab entry and
refreshed by `Ctrl+R`.

**`ToggleFavorite` (`f`)** is the optimistic-update reference implementation for the whole app:
1. Flip `is_favorite` on the item in state immediately.
2. Emit `Effect::Net(SetFavorite { id, on })`.
3. In the Favourites tab, unfavouriting also **removes the row** immediately.
4. On `Data::LoadFailed` for that request: restore the flag, reinsert the row at its original index,
   and toast `could not update favourite: <reason>`.

Restoring to the *original index* rather than appending matters — a user who unfavourites by
accident and sees the row jump to the bottom will think something else broke.

The flag must be updated **everywhere the item appears**: the favourites list, any Miller column
holding it, the queue, and the inspector. Implement a helper
`AppState::update_item_favorite(&mut self, id: &ItemId, on: bool)` that walks all of them; leaving
one stale is the obvious bug here.

**Empty state:** `no favourites yet — press {f} on anything to add it`, with the key from the keymap.

**Offline:** the tab renders from cached data when available and otherwise shows the offline empty
state. `f` is refused with `favourites need a connection`.

## Acceptance
- `favourites_splits_into_three_sections`
- `toggle_favorite_updates_optimistically`
- `unfavourite_removes_row_immediately`
- `failure_restores_flag_and_row_at_original_index`
- `favorite_flag_updated_in_all_views` — an item present in a Miller column, the queue, and
  favourites; all three reflect the change.
- `empty_state_shows_keymap_hint`
- `favorite_refused_when_offline`
- `ctrl_r_refreshes`
- `favourites_snapshot`, `favourites_snapshot_empty`

## Done when
The global DoD in `tasks/README.md` is satisfied.
