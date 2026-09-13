//! Reducer: queue mutations and playback advance (`06-01`), the appears-on queue rules (`06-02`),
//! non-destructive shuffle (`06-03`), sort profiles (`06-04`), and gapless preloading (`06-06`).
//!
//! Scope: the queue *mechanics* — append, insert, remove, reorder, advance, repeat, next/prev —
//! operating on tracks already available in `state` (a loaded Tracks/ArtistTracks/PlaylistTracks/
//! SearchResults column, or an explicit selection within one), plus resolving a single focused
//! `Artist`/`Album` row into its tracks: a network fetch (`FetchAlbumTracksForQueue`/
//! `FetchArtistTracksForQueue`), with the `ArtistOnly` filter applied once
//! `DataAction::TracksLoaded` replies (`apply_tracks_loaded`). A *multi-selection* containing
//! container rows, and `i` (insert-next) on a container, both need several such fetches
//! reassembled in selection order and remembered as insert-vs-append — that is what
//! [`crate::state::queue::QueueBatch`] is for.
//!
//! **Stream URL resolution is not implemented here, and no task in the visible library explicitly
//! owns it.** Resolving a real playable URL needs `POST /Items/{id}/PlaybackInfo` plus
//! `stream::StreamUrl::build` (`docs/03-emby-api.md` §5) — both live in `loxia-emby`, which
//! `loxia-core` cannot depend on, and both require an actual network round trip this pure reducer
//! has no way to perform. Every `Effect::Audio(Load/Preload)` this module emits carries a
//! placeholder `emby-track:{id}` URL, clearly not a real HTTP URL, so at least nothing here
//! silently pretends to have solved a problem it hasn't. See `docs/12-decisions.md`.

use crate::action::{Action, ItemAction, QueueAction};
use crate::effect::{AudioEffect, CacheEffect, Effect, NetEffect, SysEffect, TrackChange};
use crate::model::{AlbumRelation, ItemId, MediaItem, PlaylistEntryId, PlaylistId, Track};
use crate::queue::appears_on::{TrackFilter, filter_tracks};
use crate::queue::shuffle;
use crate::queue::sort;
use crate::state::nav::{Column, ColumnKind, Tab};
use crate::state::player::PlayStatus;
use crate::state::queue::{
    Availability, QueueBatch, QueueBatchMode, QueueBatchSlot, QueueEntry, QueueSource, RepeatMode,
};
use crate::state::search::SearchSection;
use crate::state::toast::ToastLevel;
use crate::state::{
    AppState, Connectivity, PendingFavoriteToggle, PendingPlaylistMutation, RemovedFavourite,
};

/// How far into the current track `Prev` must be to restart it instead of moving back — this
/// task's own spec: "the behaviour every music player has and users expect without being told".
const PREV_RESTART_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(3);

pub fn apply_queue(state: &mut AppState, action: QueueAction) -> Vec<Effect> {
    let mut effects = match action {
        QueueAction::QueueSelection { full_context } => queue_selection(state, full_context),
        QueueAction::PlaySelection { full_context } => play_selection(state, full_context),
        QueueAction::InsertNext => insert_next(state),
        QueueAction::RemoveEntry => remove_current_entry(state),
        QueueAction::MoveEntry { from, to } => move_entry(state, from, to),
        QueueAction::Clear => clear(state),
        QueueAction::CycleRepeat => cycle_repeat(state),
        QueueAction::JumpTo(id) => jump_to(state, id),
        QueueAction::ToggleShuffle { seed } => toggle_shuffle(state, seed),
        QueueAction::ApplySortProfile(name) => apply_sort_profile(state, &name),
        QueueAction::RestoreDefaultOrder => restore_default_order(state),
        QueueAction::InstantMix => instant_mix(state),
        QueueAction::RequeueTrack(track) => {
            append_tracks(state, vec![(*track, QueueSource::Manual)])
        }
    };
    // `06-06`: every one of the branches above can change what's at `position + 1` (or, under
    // `Repeat::One`, the current entry itself) — recomputed and deduped unconditionally here
    // rather than threaded individually through each branch, since `preload_effects` itself is the
    // thing that decides whether anything actually changed.
    effects.extend(preload_effects(state));
    debug_assert_invariants(state);
    effects
}

/// `07-02`: `ToggleFavorite` is the only `ItemAction` this task's own scope covers —
/// `ToggleDownload`/`AddToPlaylist`/etc. are later tasks' own jobs, no-op here rather than
/// guessing at their logic.
pub fn apply_item(state: &mut AppState, action: ItemAction) -> Vec<Effect> {
    match action {
        ItemAction::ToggleFavorite => toggle_favorite(state),
        ItemAction::RemoveFromPlaylist { playlist, entries } => {
            remove_from_playlist(state, playlist, entries)
        }
        ItemAction::MoveInPlaylist {
            playlist,
            entry,
            new_index,
        } => move_in_playlist(state, playlist, entry, new_index),
        ItemAction::DeletePlaylist(id) => confirm_delete_playlist(state, id),
        ItemAction::DeletePlaylistConfirmed(id) => delete_playlist(state, id),
        ItemAction::ToggleDownload => toggle_download(state),
        // `AddToPlaylist` opens the `SavePlaylist` modal (`10-08`, not yet built) — no-op here
        // rather than guessing at a modal this task doesn't own (`docs/12-decisions.md`).
        ItemAction::AddToPlaylist { .. } => Vec::new(),
        ItemAction::SaveQueueAsPlaylist { .. } => Vec::new(),
    }
}

/// The optimistic-update reference implementation for the whole app (`07-02`): flip the flag
/// everywhere immediately (`AppState::update_item_favorite`), additionally remove the row from
/// the Favourites tab's own list if unfavouriting from there, then emit the real request —
/// `favorite_toggle_failed` (below) is what rolls all of this back if it fails.
fn toggle_favorite(state: &mut AppState) -> Vec<Effect> {
    if state.connectivity == Connectivity::Offline {
        state.toast("favourites need a connection", ToastLevel::Warning);
        return Vec::new();
    }
    let Some((id, currently_favorite)) = resolve_favorite_target(state) else {
        return Vec::new();
    };
    let on = !currently_favorite;

    let removed_from_favourites = if state.nav.active_tab == Tab::Favourites && !on {
        remove_from_favourites(state, &id)
    } else {
        None
    };
    state.update_item_favorite(&id, on);
    // `update_item_favorite` flips the flag wherever the item *already appears*; it cannot add a
    // newly-favourited track to a list that has never held it. The Favourites tab loads once, on
    // first entry (`LoadState::Idle`), and nothing invalidated it — so a track favourited from a
    // Miller column showed its heart there and then never turned up in Favourites, however many
    // times you visited the tab (`docs/12-decisions.md`).
    //
    // Marked stale rather than patched in place: the server decides what is in that list and in
    // what order, and a locally inserted row would be a guess at both. Unfavouriting *from* the tab
    // is the one case that must not do this — `remove_from_favourites` has already taken the row
    // out, and a refetch would only make the list flicker.
    if state.nav.active_tab != Tab::Favourites {
        state.favourites.load = crate::state::nav::LoadState::Idle;
    }
    state.touch();

    state.pending_favorite_toggles.insert(
        id.clone(),
        PendingFavoriteToggle {
            on,
            removed_from_favourites,
        },
    );
    vec![Effect::Net(NetEffect::SetFavorite { id, on })]
}

/// The id + current flag of whatever's focused, checked in the same tab-dependent order the
/// queueing/instant-mix helpers already use: the Search tab's focused result, the Favourites
/// tab's focused result, or (every other tab) the active Miller column's selected item.
fn resolve_favorite_target(state: &AppState) -> Option<(ItemId, bool)> {
    let item = if is_sectioned_tab(state.nav.active_tab) {
        focused_section_item(state)?
    } else if let Some(track) = focused_play_view_track(state) {
        MediaItem::Track(track)
    } else {
        state.selected_item()?.clone()
    };
    let id = item.id()?.clone();
    let favorite = match &item {
        MediaItem::Artist(a) => a.is_favorite,
        MediaItem::Album(a) => a.is_favorite,
        MediaItem::Track(t) => t.is_favorite,
        MediaItem::Playlist(p) => p.is_favorite,
        // A genre is not an item Emby can favourite, and a folder is a filesystem node, not a
        // library item at all.
        _ => return None,
    };
    Some((id, favorite))
}

/// The track a *play view* is about: the row under the Now Playing cursor, or — in Zen, which has
/// no cursor of its own — whatever is playing.
///
/// Neither view has a Miller column, so `AppState::selected_item` returns `None` on both and `f`
/// did nothing at all there: you could not favourite the track you were listening to from the two
/// screens that exist to show it (`docs/12-decisions.md`). The same gap the Favourites tab had.
///
/// `None` for every other tab, so nothing else changes.
fn focused_play_view_track(state: &AppState) -> Option<Track> {
    // Zen renders over whichever tab is active, and shows one track — the playing one.
    if state.zen_mode {
        return state.queue.current().map(|entry| entry.track.clone());
    }
    if state.nav.active_tab != Tab::NowPlaying {
        return None;
    }
    match state.now_playing_subview {
        crate::state::NowPlayingSub::Queue => state
            .queue
            .play_order
            .get(state.now_playing_cursor)
            .and_then(|&index| state.queue.entries.get(index))
            .map(|entry| entry.track.clone())
            .or_else(|| state.queue.current().map(|entry| entry.track.clone())),
        crate::state::NowPlayingSub::History => state
            .history_sorted()
            .get(state.now_playing_cursor)
            .map(|entry| entry.track.clone()),
    }
}

/// Removes `id`'s row from `state.favourites.results`, if present, recording its exact position
/// so `favorite_toggle_failed` can restore it there rather than appending it back at the end.
fn remove_from_favourites(state: &mut AppState, id: &ItemId) -> Option<RemovedFavourite> {
    let results = &mut state.favourites.results;
    if let Some(pos) = results.artists.iter().position(|a| &a.id == id) {
        return Some(RemovedFavourite::Artist(pos, results.artists.remove(pos)));
    }
    if let Some(pos) = results.albums.iter().position(|a| &a.id == id) {
        return Some(RemovedFavourite::Album(pos, results.albums.remove(pos)));
    }
    if let Some(pos) = results.tracks.iter().position(|t| &t.id == id) {
        return Some(RemovedFavourite::Track(pos, results.tracks.remove(pos)));
    }
    if let Some(pos) = results.playlists.iter().position(|p| &p.id == id) {
        return Some(RemovedFavourite::Playlist(
            pos,
            results.playlists.remove(pos),
        ));
    }
    None
}

fn reinsert_removed_favourite(state: &mut AppState, removed: RemovedFavourite) {
    let results = &mut state.favourites.results;
    match removed {
        RemovedFavourite::Artist(pos, a) => {
            results.artists.insert(pos.min(results.artists.len()), a);
        }
        RemovedFavourite::Album(pos, a) => {
            results.albums.insert(pos.min(results.albums.len()), a);
        }
        RemovedFavourite::Track(pos, t) => {
            results.tracks.insert(pos.min(results.tracks.len()), t);
        }
        RemovedFavourite::Playlist(pos, p) => {
            results
                .playlists
                .insert(pos.min(results.playlists.len()), p);
        }
    }
}

/// `DataAction::LoadFailed { target: LoadTarget::FavoriteToggle(id), .. }` — restores the flag
/// (back to whatever it was *before* the optimistic flip) and, if this toggle had removed a row
/// from the Favourites list, reinserts it at its original index.
pub(crate) fn favorite_toggle_failed(
    state: &mut AppState,
    id: ItemId,
    message: String,
) -> Vec<Effect> {
    let Some(pending) = state.pending_favorite_toggles.remove(&id) else {
        return Vec::new();
    };
    state.update_item_favorite(&id, !pending.on);
    if let Some(removed) = pending.removed_from_favourites {
        reinsert_removed_favourite(state, removed);
    }
    state.toast(
        format!("could not update favourite: {message}"),
        ToastLevel::Error,
    );
    state.touch();
    Vec::new()
}

// --- 07-03: playlist mutations (remove/reorder/delete), all optimistic with rollback ---------

/// The `Playlists` list column itself, if the tab has been visited (depth 0, always present
/// ahead of any drilled-in `PlaylistTracks` column).
fn playlists_column_mut(state: &mut AppState) -> Option<&mut Column> {
    state
        .nav
        .per_tab_stacks
        .get_mut(&Tab::Playlists)?
        .iter_mut()
        .find(|c| c.kind == ColumnKind::Playlists)
}

/// The `PlaylistTracks` column for this specific playlist, if it's currently loaded (the user
/// has drilled into it).
fn playlist_tracks_column_mut<'a>(
    state: &'a mut AppState,
    playlist: &PlaylistId,
) -> Option<&'a mut Column> {
    state
        .nav
        .per_tab_stacks
        .get_mut(&Tab::Playlists)?
        .iter_mut()
        .find(|c| matches!(&c.kind, ColumnKind::PlaylistTracks { of_playlist } if of_playlist.as_str() == playlist.as_str()))
}

/// `x` on a `PlaylistTracks` row — removes the focused track, or every selected track in
/// visual-select mode, in one request (`multiselect_remove_sends_one_request`). Optimistic:
/// removed from the column immediately, restored (at each row's own original index, not
/// appended) if `PlaylistRemove` fails.
fn remove_from_playlist(
    state: &mut AppState,
    playlist: PlaylistId,
    entries: Vec<PlaylistEntryId>,
) -> Vec<Effect> {
    if state.connectivity == Connectivity::Offline {
        state.toast("playlist changes need a connection", ToastLevel::Warning);
        return Vec::new();
    }
    if entries.is_empty() {
        return Vec::new();
    }
    let Some(column) = playlist_tracks_column_mut(state, &playlist) else {
        return Vec::new();
    };

    let mut removed = Vec::new();
    let mut index = 0usize;
    column.items.retain(|item| {
        let matches = match item {
            MediaItem::Track(t) => t
                .playlist_entry_id
                .as_ref()
                .is_some_and(|id| entries.contains(id)),
            _ => false,
        };
        if matches {
            removed.push((index, item.clone()));
        }
        index += 1;
        !matches
    });
    if removed.is_empty() {
        return Vec::new();
    }
    column.cursor = column.cursor.min(column.items.len().saturating_sub(1));
    state.touch();

    state
        .pending_playlist_mutations
        .insert(playlist.clone(), PendingPlaylistMutation::Remove(removed));
    vec![Effect::Net(NetEffect::PlaylistRemove {
        id: playlist,
        entries,
    })]
}

/// `Ctrl+Up`/`Ctrl+Down` — moves the focused track one position within its playlist. Optimistic:
/// reordered immediately, moved back to `from_index` if `PlaylistMove` fails.
fn move_in_playlist(
    state: &mut AppState,
    playlist: PlaylistId,
    entry: PlaylistEntryId,
    new_index: usize,
) -> Vec<Effect> {
    if state.connectivity == Connectivity::Offline {
        state.toast("playlist changes need a connection", ToastLevel::Warning);
        return Vec::new();
    }
    let Some(column) = playlist_tracks_column_mut(state, &playlist) else {
        return Vec::new();
    };
    let Some(from_index) = column.items.iter().position(
        |item| matches!(item, MediaItem::Track(t) if t.playlist_entry_id.as_ref() == Some(&entry)),
    ) else {
        return Vec::new();
    };
    let new_index = new_index.min(column.items.len().saturating_sub(1));
    if new_index == from_index {
        return Vec::new();
    }
    let item = column.items.remove(from_index);
    let Some(item_id) = item.id().cloned() else {
        // Never actually reachable (the position search above only ever matches a `Track`,
        // which always has an id), but restores the removed row rather than silently dropping
        // it if it somehow were.
        column.items.insert(from_index, item);
        return Vec::new();
    };
    column.items.insert(new_index, item);
    column.cursor = new_index;
    state.touch();

    state.pending_playlist_mutations.insert(
        playlist.clone(),
        PendingPlaylistMutation::Move { entry, from_index },
    );
    vec![Effect::Net(NetEffect::PlaylistMove {
        id: playlist,
        item: item_id,
        new_index,
    })]
}

/// `X` — never deletes directly; opens a `Confirm` modal naming the playlist and its track
/// count, whose `on_confirm` is `DeletePlaylistConfirmed` (`docs/12-decisions.md`).
fn confirm_delete_playlist(state: &mut AppState, id: PlaylistId) -> Vec<Effect> {
    let Some(column) = playlists_column_mut(state) else {
        return Vec::new();
    };
    let Some(MediaItem::Playlist(playlist)) = column.items.iter().find(|item| {
        item.id()
            .is_some_and(|item_id| item_id.as_str() == id.as_str())
    }) else {
        return Vec::new();
    };
    let prompt = format!(
        "Delete playlist \"{}\" ({} tracks)? This cannot be undone.",
        playlist.name, playlist.track_count
    );
    crate::reducer::modal::open_confirm(
        state,
        prompt,
        Action::Item(ItemAction::DeletePlaylistConfirmed(id)),
    )
}

/// The `Confirm` modal's own submit target — optimistically removes the playlist row from the
/// `Playlists` list column, restored at its original index if `PlaylistDelete` fails.
fn delete_playlist(state: &mut AppState, id: PlaylistId) -> Vec<Effect> {
    if state.connectivity == Connectivity::Offline {
        state.toast("playlist changes need a connection", ToastLevel::Warning);
        return Vec::new();
    }
    let Some(column) = playlists_column_mut(state) else {
        return Vec::new();
    };
    let Some(index) = column.items.iter().position(|item| {
        item.id()
            .is_some_and(|item_id| item_id.as_str() == id.as_str())
    }) else {
        return Vec::new();
    };
    let removed = column.items.remove(index);
    column.cursor = column.cursor.min(column.items.len().saturating_sub(1));
    state.touch();

    state
        .pending_playlist_mutations
        .insert(id.clone(), PendingPlaylistMutation::Delete(index, removed));
    vec![Effect::Net(NetEffect::PlaylistDelete { id })]
}

/// `DataAction::LoadFailed { target: LoadTarget::PlaylistMutation(id), .. }` — rolls back
/// whichever of the three mutations was pending for this playlist.
pub(crate) fn playlist_mutation_failed(
    state: &mut AppState,
    id: PlaylistId,
    message: String,
) -> Vec<Effect> {
    let Some(pending) = state.pending_playlist_mutations.remove(&id) else {
        return Vec::new();
    };
    match pending {
        PendingPlaylistMutation::Remove(removed) => {
            if let Some(column) = playlist_tracks_column_mut(state, &id) {
                for (index, item) in removed {
                    column.items.insert(index.min(column.items.len()), item);
                }
            }
        }
        PendingPlaylistMutation::Move { entry, from_index } => {
            if let Some(column) = playlist_tracks_column_mut(state, &id)
                && let Some(current_index) = column.items.iter().position(|item| {
                    matches!(item, MediaItem::Track(t) if t.playlist_entry_id.as_ref() == Some(&entry))
                })
            {
                let item = column.items.remove(current_index);
                let restore_at = from_index.min(column.items.len());
                column.items.insert(restore_at, item);
            }
        }
        PendingPlaylistMutation::Delete(index, item) => {
            if let Some(column) = playlists_column_mut(state) {
                column.items.insert(index.min(column.items.len()), item);
            }
        }
    }
    state.toast(
        format!("playlist change failed: {message}"),
        ToastLevel::Error,
    );
    state.touch();
    Vec::new()
}

/// `Audio(TrackEnded)` — called from `reducer::player::apply_audio`, which owns the rest of the
/// `Action::Audio` dispatch. `natural: false` (explicit stop/skip that already moved the position
/// itself) never advances.
pub fn advance_on_track_ended(state: &mut AppState, natural: bool) -> Vec<Effect> {
    if !natural || state.queue.entries.is_empty() {
        return Vec::new();
    }
    let mut effects = match state.queue.repeat {
        RepeatMode::One => load_current(state),
        _ => {
            let at_end = state.queue.position + 1 >= state.queue.play_order.len();
            if !at_end {
                state.queue.position += 1;
                load_current(state)
            } else if state.queue.repeat == RepeatMode::All {
                state.queue.position = 0;
                load_current(state)
            } else {
                state.player.status = PlayStatus::Stopped;
                Vec::new()
            }
        }
    };
    state.touch();
    effects.extend(preload_effects(state));
    debug_assert_invariants(state);
    effects
}

/// `PlayerAction::Next` — always advances, even under `Repeat::One` (an explicit skip is not the
/// same request as "replay this on natural end").
pub fn next(state: &mut AppState) -> Vec<Effect> {
    if state.queue.entries.is_empty() {
        return Vec::new();
    }
    let at_end = state.queue.position + 1 >= state.queue.play_order.len();
    let mut effects = if !at_end {
        state.queue.position += 1;
        load_current(state)
    } else if state.queue.repeat == RepeatMode::All {
        state.queue.position = 0;
        load_current(state)
    } else {
        state.player.status = PlayStatus::Stopped;
        Vec::new()
    };
    state.touch();
    effects.extend(preload_effects(state));
    debug_assert_invariants(state);
    effects
}

/// `PlayerAction::Prev` — restarts the current track from 0 once played past
/// `PREV_RESTART_THRESHOLD`; otherwise moves back one entry.
pub fn prev(state: &mut AppState) -> Vec<Effect> {
    if state.queue.entries.is_empty() {
        return Vec::new();
    }
    let mut effects = if state.player.position >= PREV_RESTART_THRESHOLD {
        load_current(state)
    } else if state.queue.position > 0 {
        state.queue.position -= 1;
        load_current(state)
    } else {
        load_current(state)
    };
    state.touch();
    effects.extend(preload_effects(state));
    debug_assert_invariants(state);
    effects
}

/// `06-03`/`06-06`: toggles shuffle on or off; `preload_effects` (called uniformly by
/// `apply_queue` for every `QueueAction`) picks up whatever is now next. `seed` is supplied by the
/// caller (`docs/12-decisions.md`: derived from `state.clock` at the input-mapping layer, never
/// read from a clock here).
fn toggle_shuffle(state: &mut AppState, seed: u64) -> Vec<Effect> {
    if state.queue.shuffled {
        shuffle::unshuffle(&mut state.queue);
    } else {
        shuffle::shuffle(&mut state.queue, seed);
    }
    state.touch();
    Vec::new()
}

/// The entry that should be preloaded right now: `position + 1`, except under `Repeat::One`
/// (the current entry itself — it's about to replay), wrapping to index `0` under `Repeat::All`
/// at the end, and `None` under `Repeat::Off` at the end (nothing plays next).
pub fn preload_target(q: &crate::state::queue::QueueState) -> Option<&QueueEntry> {
    if q.play_order.is_empty() {
        return None;
    }
    let target_slot = if q.repeat == RepeatMode::One {
        q.position
    } else {
        let next = q.position + 1;
        if next < q.play_order.len() {
            next
        } else if q.repeat == RepeatMode::All {
            0
        } else {
            return None;
        }
    };
    let &index = q.play_order.get(target_slot)?;
    q.entries.get(index)
}

/// `06-06`: recomputes [`preload_target`] and, only if it's genuinely different from the last
/// entry preloaded (`PlayerState::last_preloaded`) and actually queueable (not `Unavailable`, and
/// not `Remote` while offline — reusing the same [`is_queueable`] gate `06-01`'s appending already
/// uses), emits `Effect::Audio(Preload)`. Deduping here — rather than requiring every call site to
/// know whether *it specifically* changed the next entry — is what makes calling this
/// unconditionally after every mutation (per this task's own table) cheap instead of thrashing
/// mpv's cache on every queue touch. `pub(crate)`: `09-06`'s own `reducer::player::cycle_quality`
/// clears `last_preloaded` and calls this too, since the preloaded next entry was queued at the
/// old profile.
pub(crate) fn preload_effects(state: &mut AppState) -> Vec<Effect> {
    let Some(entry) = preload_target(&state.queue) else {
        state.player.last_preloaded = None;
        return Vec::new();
    };
    if !can_preload(entry.availability, state.connectivity) {
        return Vec::new();
    }
    if state.player.last_preloaded == Some(entry.entry_id) {
        return Vec::new();
    }
    let entry_id = entry.entry_id;
    let url = placeholder_url(&entry.track.id);
    state.player.last_preloaded = Some(entry_id);
    let headers = stream_headers(state, &url);
    let mut effects = vec![Effect::Audio(AudioEffect::Preload {
        url,
        headers,
        gain_db: None,
    })];
    effects.extend(prefetch_effects(state));
    effects
}

/// `cache.prefetch_next`: pulls the next N upcoming entries into the rolling cache at the profile
/// the current track is playing at, so skipping forward — or losing the connection mid-album —
/// finds them already local. mpv's own `Preload` above only ever reaches *one* track ahead and
/// keeps nothing on disk, so before this the cache only ever held what had actually been played.
///
/// Emitted as one effect carrying the whole run: the cache worker owns a single in-flight fetch
/// slot keyed by track, and N separate `EnsureCached`es would cancel one another and the current
/// track's own fetch (`docs/06-cache-and-offline.md` §4).
///
/// Skipped entirely when the cache is off (nowhere to put them), while offline (nothing to fetch
/// from), or when the run would be empty. Entries already local, or unavailable, are left out
/// rather than making the worker discover that per track.
fn prefetch_effects(state: &AppState) -> Vec<Effect> {
    let count = usize::from(state.config.cache.prefetch_next);
    if !state.config.cache.enabled || count == 0 || state.connectivity != Connectivity::Online {
        return Vec::new();
    }

    // Straight ahead in play order from the entry after the current one. Deliberately does *not*
    // wrap under `Repeat::All` the way `preload_target` does: at the end of a queue the tracks
    // "ahead" are ones already played, and so already cached.
    let tracks: Vec<crate::model::Track> = state
        .queue
        .play_order
        .iter()
        .skip(state.queue.position + 1)
        .filter_map(|&index| state.queue.entries.get(index))
        .filter(|entry| entry.availability == Availability::Remote)
        .take(count)
        .map(|entry| entry.track.clone())
        .collect();

    if tracks.is_empty() {
        return Vec::new();
    }
    vec![Effect::Cache(Box::new(CacheEffect::PrefetchAhead {
        tracks,
        profile: state.player.quality_profile,
    }))]
}

/// `06-04`: rebuilds `entries` in sorted order, then rebuilds `play_order` back to the identity
/// and repositions to keep the current track playing — sorting and shuffling are mutually
/// exclusive views of ordering, so this always clears `shuffled` too. An unknown profile name
/// (e.g. one just deleted from config) is a silent no-op, matching this reducer's own rule that a
/// bad reference never panics.
/// `pub(crate)` (`11-04`): the sort-profile editor's own "Apply" reuses this exact function
/// rather than re-deriving its behaviour.
pub(crate) fn apply_sort_profile(state: &mut AppState, name: &str) -> Vec<Effect> {
    let Some(profile) = state
        .config
        .sorting
        .profiles
        .iter()
        .find(|p| p.name == name)
        .cloned()
    else {
        return Vec::new();
    };

    // Reorders `play_order`, never `entries`. `entries` holds the order tracks were queued in —
    // an album's disc/track order, a playlist's own order, a folder's file paths — and that *is*
    // the default order, so destroying it (as sorting `entries` in place used to) left no way back
    // from a sort (`docs/12-decisions.md`). Shuffle has always worked this way; sorting now matches
    // it, which is what makes `restore_default_order` a plain reset rather than a re-fetch.
    let current_index = state.queue.play_order.get(state.queue.position).copied();
    let mut order: Vec<usize> = (0..state.queue.entries.len()).collect();
    {
        let entries = &state.queue.entries;
        // Key the `Year` field off each album's own year (not each track's, which is often missing
        // or inconsistent) so a chronological-discography sort keeps every album together and in
        // order — the fix for "artist + year + album + track scatters years and albums"
        // (`docs/12-decisions.md`).
        let album_years = sort::album_years(entries.iter().map(|e| &e.track));
        order.sort_by(|&a, &b| {
            sort::compare_with_album_year(
                &entries[a].track,
                &entries[b].track,
                &profile,
                &album_years,
            )
        });
    }
    state.queue.play_order = order;
    state.queue.position = current_index
        .and_then(|index| state.queue.play_order.iter().position(|&i| i == index))
        .unwrap_or(0);
    state.queue.shuffled = false;
    state.queue.sort_profile = Some(profile.name);
    state.touch();
    debug_assert_invariants(state);
    Vec::new()
}

/// Puts the queue back into the order its tracks were added in — an album's own disc/track order, a
/// playlist's order, a folder's file paths — undoing a sort profile the way `unshuffle` undoes a
/// shuffle, and by the identical mechanism (`docs/12-decisions.md`).
pub(crate) fn restore_default_order(state: &mut AppState) -> Vec<Effect> {
    let current_index = state.queue.play_order.get(state.queue.position).copied();
    state.queue.play_order = (0..state.queue.entries.len()).collect();
    if let Some(index) = current_index {
        state.queue.position = index;
    }
    state.queue.shuffled = false;
    state.queue.sort_profile = None;
    state.touch();
    debug_assert_invariants(state);
    Vec::new()
}

fn cycle_repeat(state: &mut AppState) -> Vec<Effect> {
    state.queue.repeat = match state.queue.repeat {
        RepeatMode::Off => RepeatMode::All,
        RepeatMode::All => RepeatMode::One,
        RepeatMode::One => RepeatMode::Off,
    };
    state.touch();
    Vec::new()
}

/// `pub(crate)` (`11-03`): reused directly by `reducer::settings`'s server-switch confirmation,
/// which needs the exact same "stop playback, empty the queue" behaviour before it separately
/// clears `state.history` too (something an ordinary `Clear` deliberately never does,
/// `docs/06-cache-and-offline.md` §8 — switching servers is the one case that legitimately wants
/// both).
pub(crate) fn clear(state: &mut AppState) -> Vec<Effect> {
    // `06-07`: "Stopped" reported before anything about the current entry is torn down —
    // `report_stopped` needs `player.current`/`queue.entries` still intact to resolve it.
    let mut effects = crate::reducer::player::report_stopped(state);
    // `10-11`: setting `status` directly here (rather than through `AudioEvent::StatusChanged`,
    // `apply_audio`'s own sole writer for this field) bypasses `mpris_meta`'s only other call
    // site, so it's called explicitly here too, still before `entries.clear()` below — same
    // ordering requirement as `report_stopped`'s own, and for the same reason.
    state.player.status = PlayStatus::Stopped;
    effects.extend(crate::reducer::player::mpris_meta(state));
    state.queue.entries.clear();
    state.queue.play_order.clear();
    state.queue.position = 0;
    state.queue.mix_name = None;
    state.player.current = None;
    state.touch();
    effects.push(Effect::Audio(AudioEffect::Stop));
    effects
}

fn move_entry(state: &mut AppState, from: usize, to: usize) -> Vec<Effect> {
    if from >= state.queue.play_order.len() || to >= state.queue.play_order.len() || from == to {
        return Vec::new();
    }
    // The entry the playhead is *on* must stay on it even as other entries shuffle around it.
    let current_index = state.queue.play_order.get(state.queue.position).copied();

    let moved = state.queue.play_order.remove(from);
    state.queue.play_order.insert(to, moved);

    if let Some(index) = current_index
        && let Some(new_pos) = state.queue.play_order.iter().position(|&i| i == index)
    {
        state.queue.position = new_pos;
    }
    // `07-06`: the Now Playing view's own cursor follows the row it just reordered, the same
    // "stays glued to the thing you moved" rule the playhead itself gets above.
    if state.nav.active_tab == Tab::NowPlaying && state.now_playing_cursor == from {
        state.now_playing_cursor = to;
    }
    state.touch();
    Vec::new()
}

fn jump_to(state: &mut AppState, id: crate::model::QueueEntryId) -> Vec<Effect> {
    let Some(index) = state.queue.entries.iter().position(|e| e.entry_id == id) else {
        return Vec::new();
    };
    let Some(play_pos) = state.queue.play_order.iter().position(|&i| i == index) else {
        return Vec::new();
    };
    state.queue.position = play_pos;
    let effects = load_current(state);
    state.touch();
    debug_assert_invariants(state);
    effects
}

fn remove_current_entry(state: &mut AppState) -> Vec<Effect> {
    // `docs/04-state-and-input.md` §4 says `RemoveEntry`'s target is "resolved from `nav.focus`" —
    // `07-06` finally gives the `NowPlaying` tab its own cursor to resolve that from (`nav.focus`
    // itself is still `NavFocus::Sidebar`/never a column there, `seed_column_for_tab` excludes it,
    // so `now_playing_cursor` is what actually stands in for it). History is read-only (`x` is a
    // no-op there); every other tab keeps removing the currently-*playing* entry, the only
    // unambiguous interpretation available without a cursor of its own. See `docs/12-decisions.md`.
    let removed_index = if state.nav.active_tab == Tab::NowPlaying {
        if state.now_playing_subview != crate::state::NowPlayingSub::Queue {
            return Vec::new();
        }
        state
            .queue
            .play_order
            .get(state.now_playing_cursor)
            .copied()
    } else {
        state.queue.play_order.get(state.queue.position).copied()
    };
    let Some(removed_index) = removed_index else {
        return Vec::new();
    };
    let effects = remove_by_index(state, removed_index);
    if state.nav.active_tab == Tab::NowPlaying {
        state.now_playing_cursor = state
            .now_playing_cursor
            .min(state.queue.play_order.len().saturating_sub(1));
    }
    effects
}

fn remove_by_index(state: &mut AppState, removed_index: usize) -> Vec<Effect> {
    let was_current = state.queue.play_order.get(state.queue.position) == Some(&removed_index);

    // The play being interrupted is closed out *before* its entry leaves `queue.entries` —
    // `report_stopped` resolves the item by looking it up there, so afterwards there is nothing
    // left to report about. Same ordering requirement `clear` already documents.
    let mut effects = if was_current {
        crate::reducer::player::report_stopped(state)
    } else {
        Vec::new()
    };

    state.queue.entries.remove(removed_index);
    // Renumber: every stored index above the removed one shifts down by one, and the removed
    // index itself drops out of `play_order` entirely.
    let removed_play_pos = state
        .queue
        .play_order
        .iter()
        .position(|&i| i == removed_index);
    state.queue.play_order.retain(|&i| i != removed_index);
    for i in state.queue.play_order.iter_mut() {
        if *i > removed_index {
            *i -= 1;
        }
    }

    if let Some(removed_play_pos) = removed_play_pos
        && removed_play_pos < state.queue.position
    {
        state.queue.position = state.queue.position.saturating_sub(1);
    }
    state.queue.position = state
        .queue
        .position
        .min(state.queue.play_order.len().saturating_sub(1));

    state.touch();
    if was_current && !state.queue.entries.is_empty() {
        effects.extend(load_current(state));
    } else if state.queue.entries.is_empty() {
        // Removing the last entry used to update the *mirror* only — `current = None`, status
        // `Stopped` — and emit no effect at all, so the player bar read "nothing playing" while
        // mpv carried on playing the track (`docs/12-decisions.md`). The engine has to be told,
        // and the rest of the now-meaningless play state torn down with it, exactly as `clear`
        // does for the whole queue.
        state.player.status = PlayStatus::Stopped;
        effects.extend(crate::reducer::player::mpris_meta(state));
        state.player.current = None;
        state.player.position = std::time::Duration::ZERO;
        state.player.playback_source = None;
        // Nothing is left to resume, so this must not read as a restored-but-unloaded session.
        state.player.restored_unloaded = false;
        state.history_recorded_this_play = false;
        effects.push(Effect::Audio(AudioEffect::Stop));
    }
    debug_assert_invariants(state);
    effects
}

/// `11-06`: the first real engine `Load` for a session restored **paused**
/// (`state.player.restored_unloaded`) — `session::restore` never eagerly loads anything into mpv
/// when `ui.restore_autoplay` is off (auto-loading on launch would seize the audio device just as
/// surely as auto-*playing* would), so the next `PlayerAction::PlayPause` after such a restore
/// takes this path instead of a bare toggle, which would have nothing to un-pause. Also called
/// directly by `session::restore`'s own `restore_autoplay = true` branch, which needs the
/// identical `Load` immediately rather than waiting for a keypress. Unlike `load_current`,
/// `state.player.position` is preserved rather than reset to zero — this is a resume, not a fresh
/// play — and becomes the `Load`'s `start_at`.
/// The lyrics fetch for `track`, or nothing when the pane is off, the track has no lyric stream, or
/// the lyrics already on screen are this exact track's (a `Repeat::One` replay needs no re-fetch).
///
/// Shared by `load_current` and `resume_after_restore`: a restored session sets `player.current`
/// without going through `load_current` at all, so a restored track used to fetch nothing — the pane
/// then sat on "loading lyrics…" indefinitely, with no request ever issued to explain it
/// (`docs/12-decisions.md`).
fn lyrics_fetch_for(state: &AppState, track: &Track) -> Vec<Effect> {
    if !state.config.ui.show_lyrics {
        return Vec::new();
    }
    if state.lyrics.as_ref().is_some_and(|(id, _)| *id == track.id) {
        return Vec::new();
    }
    let Some(stream_ref) = track.lyric_stream.clone() else {
        return Vec::new();
    };
    vec![Effect::Net(NetEffect::FetchLyrics {
        track: track.id.clone(),
        stream_ref,
    })]
}

pub(crate) fn resume_after_restore(state: &mut AppState) -> Vec<Effect> {
    let Some(entry) = state.queue.current().cloned() else {
        return Vec::new();
    };
    state.player.session = Some(state.player.next_session_id());
    state.player.play_reported = false;
    state.history_recorded_this_play = false;
    let start_at = state.player.position;
    let mut effects = vec![
        Effect::Cache(Box::new(CacheEffect::EnsureCached {
            track: entry.track.clone(),
            profile: state.player.quality_profile,
        })),
        Effect::Audio(AudioEffect::Load {
            headers: stream_headers(state, &placeholder_url(&entry.track.id)),
            url: placeholder_url(&entry.track.id),
            start_at,
            gain_db: None,
        }),
    ];
    effects.extend(lyrics_fetch_for(state, &entry.track));
    effects
}

/// Builds the placeholder `Effect::Audio(Load)` for whatever is now at `queue.position`, and
/// records it as `player.current` — the mirror the UI actually reads for "what's playing".
fn load_current(state: &mut AppState) -> Vec<Effect> {
    let Some(entry) = state.queue.current().cloned() else {
        return Vec::new();
    };
    state.player.current = Some(entry.entry_id);
    state.player.position = std::time::Duration::ZERO;
    state.player.status = PlayStatus::Loading;
    // Known from the item metadata before a byte is fetched, so the seek bar is right from the
    // first frame rather than inheriting the previous track's length until mpv reports
    // (`reducer::player::authoritative_duration`).
    state.player.duration = entry.track.duration;
    // A fresh play: the next `Playing` announces it to the server (`PlayerState::start_reported`).
    state.player.start_reported = false;
    // Unknown again until this track's own cache lookup replies — the previous track's answer
    // says nothing about this one.
    state.player.playback_source = None;
    // A fresh play begins here — `06-05`'s history-completion guard resets so `Repeat::One`
    // replaying this same entry can be recorded again.
    state.history_recorded_this_play = false;
    // `06-07`: a fresh `PlaySessionId` per `Load` (never per entry — `Repeat::One` reloading the
    // *same* entry is still a new play, and needs a new session), and the "Played already
    // reported" guard resets alongside it.
    state.player.session = Some(state.player.next_session_id());
    state.player.play_reported = false;
    // `07-06`: every track change resumes the Now Playing view's own auto-scroll — cleared
    // unconditionally here, the one place every play-starting path (`QueueSelection`, `Next`/
    // `Prev`, auto-advance, `JumpTo`, a `Repeat::One` replay, ...) already funnels through.
    state.now_playing_user_scrolled = false;
    state.now_playing_cursor = state.queue.position;
    // A track change is the one moment the pane deliberately *does* move itself, so it centres
    // rather than nudging — every other scroll change keeps the view still under the pointer.
    state.now_playing_scroll = state
        .queue
        .position
        .saturating_sub(crate::reducer::nav::ASSUMED_VIEWPORT_ROWS / 2);
    // `09-04`: a new current track needs its own `applied_gain`/`applied_gain_db` resolved
    // against whatever `replay_gain` mode is already active — not just recomputed on a mode
    // change (`reducer::player::cycle_replay_gain`).
    super::player::apply_replay_gain(state);

    // `08-03`: always emitted before `Load` — a synchronous placeholder URL keeps playback from
    // ever waiting on this, but if the rolling cache already has a complete local copy, the
    // worker's `CacheResolved` reply (bounded to 50ms) lands almost immediately after and
    // `cache_resolved` below issues a corrective `Load` pointed at the local file instead.
    let mut effects = vec![
        Effect::Cache(Box::new(CacheEffect::EnsureCached {
            track: entry.track.clone(),
            profile: state.player.quality_profile,
        })),
        Effect::Audio(AudioEffect::Load {
            headers: stream_headers(state, &placeholder_url(&entry.track.id)),
            url: placeholder_url(&entry.track.id),
            start_at: std::time::Duration::ZERO,
            gain_db: None,
        }),
    ];
    // `07-07`: fetched lazily here — the one place a track actually *becomes* current — only when
    // the pane is enabled and the track carries a discovered lyric stream; skipped entirely if
    // `state.lyrics` already holds this exact track's lyrics (a `Repeat::One` replay of the same
    // entry needs no redundant re-fetch of what it's already showing).
    effects.extend(lyrics_fetch_for(state, &entry.track));
    if let Some(notify) = notify_track_change(state, &entry.track) {
        effects.push(notify);
    }
    // `10-11`: "emitted on track change" — the other two triggers (status change, the periodic
    // 10s position update) live in `reducer::player`/`reducer::mod` respectively, at their own
    // single points of truth for those events.
    effects.extend(super::player::mpris_meta(state));
    effects
}

/// `10-10`: builds the desktop-notification effect for a genuine track change — `load_current` is
/// the one place every play-starting path (`QueueSelection`, `Next`/`Prev`, auto-advance, `JumpTo`,
/// a `Repeat::One` replay, ...) already funnels through (see this function's own doc comment
/// above), and it is *not* called by `reducer::player::cycle_quality`'s same-track reload or by
/// `cache_resolved`'s corrective `Load` — so "no notification for a pause, resume, or seek" and "no
/// notification on session restore" (`reducer::session::restore` never calls this at all) both hold
/// with no extra guard needed here.
///
/// `None` when notifications are off, or the terminal already has focus (`Some(true)` only — an
/// unknown state notifies normally, `AppState::terminal_focused`'s own doc comment). There is no
/// artwork file path to offer yet: no task has built a disk cache for images (`02-12` only defined
/// the URL/cache-key naming scheme; `DataAction::ImageLoaded` today reports only that a decode
/// succeeded, never a path), so `art_path` is always `None` for now (`docs/12-decisions.md`). Rate
/// limiting and coalescing rapid skips are entirely `crates/loxia-player/src/workers/notify.rs`'s job —
/// this reducer reports every genuine change, unthrottled.
fn notify_track_change(state: &AppState, track: &Track) -> Option<Effect> {
    if !state.config.ui.desktop_notifications {
        return None;
    }
    if state.terminal_focused == Some(true) {
        return None;
    }
    Some(Effect::Sys(SysEffect::Notify(TrackChange {
        title: track.name.clone(),
        artist: track.artist_names.join(", "),
        album: track.album_name.clone(),
        art_path: None,
    })))
}

/// The headers mpv must send on its **own** HTTP request for a stream.
///
/// mpv fetches the audio itself; it does not share `EmbyClient`'s header map, so a server behind an
/// authenticating proxy needs the same custom headers repeated here. Without them the proxy answers
/// mpv's request with an HTML deny page, which mpv reports as `Failed to recognize file format` —
/// after first trying `ytdl_hook` on it — while every browsing request keeps working, since those
/// go through `reqwest` (`docs/12-decisions.md`).
///
/// Empty for a `file://` path out of the cache and for the inert `emby-track:` placeholder: neither
/// is an HTTP request, and attaching credentials to a local path is pointless at best.
pub(crate) fn stream_headers(
    state: &AppState,
    url: &crate::effect::RedactedUrl,
) -> std::collections::BTreeMap<String, String> {
    let raw = url.as_str();
    if !raw.starts_with("http://") && !raw.starts_with("https://") {
        return std::collections::BTreeMap::new();
    }
    state
        .config
        .servers
        .iter()
        .find(|s| s.id == state.config.active_server)
        .map(|s| s.custom_headers.clone())
        .unwrap_or_default()
}

/// `pub(crate)`: `09-06`'s own `reducer::player::cycle_quality` reload reuses this instead of
/// building a second placeholder scheme.
pub fn placeholder_url(id: &ItemId) -> crate::effect::RedactedUrl {
    crate::effect::RedactedUrl::new(format!("emby-track:{id}"))
}

/// `DataAction::CacheResolved` — the reply to `Effect::Cache(EnsureCached)` (`08-03`). Discarded
/// if `track`/`profile` no longer match the currently-playing entry and its active quality
/// profile: the user has since skipped to something else, or cycled quality, and this reply is
/// simply too late to matter. Otherwise always issues a corrective `Load`, swapping the source
/// without touching `position` (barely any time has passed since the original `Load`, bounded by
/// the worker's own 50ms resolve timeout): `path: Some(local)` points it at the complete local
/// cache copy; `path: None` points it at `stream_url`, the real Emby network stream.
///
/// `path: None` used to be treated as a no-op ("playback is already under way from whatever
/// `load_current` started it with") — but `load_current`/`resume_after_restore` only ever start
/// with `placeholder_url`'s inert `emby-track:{id}` scheme, which mpv cannot open. A track with no
/// local cache copy yet (i.e. almost any track never played before) therefore never actually
/// played at all — a real, confirmed defect found live: see `docs/12-decisions.md`.
pub(crate) fn cache_resolved(
    state: &mut AppState,
    track: ItemId,
    profile: crate::config::QualityProfile,
    path: Option<std::path::PathBuf>,
    stream_url: crate::effect::RedactedUrl,
) -> Vec<Effect> {
    let Some(entry) = state.queue.current() else {
        return Vec::new();
    };
    if entry.track.id != track || state.player.quality_profile != profile {
        return Vec::new();
    }
    // The single point that knows whether this track plays from disk or the network, so it is
    // where the player bar's own readout is decided too.
    let url = match &path {
        Some(path) => crate::effect::RedactedUrl::new(format!("file://{}", path.display())),
        None => stream_url,
    };
    state.player.playback_source = Some(match path {
        Some(_) => crate::state::player::PlaybackSource::Cache,
        None => crate::state::player::PlaybackSource::Streaming,
    });
    state.touch();
    let headers = stream_headers(state, &url);
    vec![Effect::Audio(AudioEffect::Load {
        url,
        headers,
        start_at: state.player.position,
        gain_db: None,
    })]
}

/// `d` — keep the selection permanently, or give it back if it is already kept.
///
/// This arm returned `Vec::new()` — the key was bound, the inspector offered the action and the
/// help sheet listed it, and pressing it did nothing at all, anywhere. Reported as "downloads not
/// working when hitting `d` in artists/albums views, and it doesn't work in select mode either"
/// (`docs/12-decisions.md`).
///
/// Works on whatever is focused: a track, an album, an artist, a playlist — every scope
/// `loxia_cache::downloads` already understood — and on each row of a visual multi-selection, since
/// "download these twelve albums" is the case that most wants it.
fn toggle_download(state: &mut AppState) -> Vec<Effect> {
    if state.connectivity == Connectivity::Offline {
        state.toast("downloads need a connection", ToastLevel::Warning);
        return Vec::new();
    }
    let items = download_targets(state);
    if items.is_empty() {
        return Vec::new();
    }

    let mut effects = Vec::new();
    let (mut pinned, mut removed) = (0usize, 0usize);
    for item in items {
        let Some(scope) = download_scope_for(&item) else {
            continue;
        };
        let id = scope.id();
        if state.downloads.contains(&id) {
            removed += 1;
            effects.push(Effect::Cache(Box::new(CacheEffect::RemoveDownload { id })));
        } else {
            pinned += 1;
            effects.push(Effect::Cache(Box::new(CacheEffect::PinDownload { scope })));
        }
    }

    // Downloading is slow and happens entirely off-screen, so it has to say something — this is the
    // same "an action you cannot see is indistinguishable from a no-op" lesson the append toast
    // already learned.
    match (pinned, removed) {
        (0, 0) => state.toast("nothing here can be downloaded", ToastLevel::Warning),
        (p, 0) => state.toast(
            format!("downloading {p} item{}", plural(p)),
            ToastLevel::Info,
        ),
        (0, r) => state.toast(
            format!("removed {r} download{}", plural(r)),
            ToastLevel::Info,
        ),
        (p, r) => state.toast(format!("downloading {p}, removed {r}"), ToastLevel::Info),
    }
    state.touch();
    effects
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The rows `d` acts on: every selected row of a visual multi-selection, else the focused one —
/// the same shape every other item action here already follows.
fn download_targets(state: &AppState) -> Vec<MediaItem> {
    if let Some(column) = state.active_column()
        && column.selection.visual_mode
        && !column.selection.selected.is_empty()
    {
        return column
            .items
            .iter()
            .filter(|item| {
                item.id()
                    .is_some_and(|id| column.selection.selected.contains(id))
            })
            .cloned()
            .collect();
    }
    focused_container_item(state).into_iter().collect()
}

fn download_scope_for(item: &MediaItem) -> Option<crate::effect::DownloadScope> {
    match item {
        MediaItem::Track(t) => Some(crate::effect::DownloadScope::Track(Box::new(t.clone()))),
        MediaItem::Album(a) => Some(crate::effect::DownloadScope::Album(a.id.clone())),
        MediaItem::Artist(a) => Some(crate::effect::DownloadScope::Artist(a.id.clone())),
        MediaItem::Playlist(p) => Some(crate::effect::DownloadScope::Playlist(PlaylistId::from(
            p.id.as_str(),
        ))),
        // A folder or genre has no download scope of its own — `loxia_cache::downloads` has no way
        // to expand either, so offering it would only fail later.
        _ => None,
    }
}

/// A background cache fetch began for `track`: while it runs the player bar says `caching` rather
/// than `stream`, so a user who turned caching on can see it working.
pub(crate) fn cache_fetch_started(
    state: &mut AppState,
    track: ItemId,
    profile: crate::config::QualityProfile,
) -> Vec<Effect> {
    if !is_current_play(state, &track, profile) {
        return Vec::new();
    }
    if state.player.playback_source == Some(crate::state::player::PlaybackSource::Streaming) {
        state.player.playback_source = Some(crate::state::player::PlaybackSource::Caching);
        state.touch();
    }
    Vec::new()
}

/// A background cache fetch finished cleanly.
///
/// Two things follow. The player bar stops saying the track is being streamed — the local copy
/// exists now, even though *this* play is still reading the bytes it already has in flight; the
/// question a user is asking of that readout is "is this cached?", not "which file descriptor is
/// mpv holding". And the About view's cache size updates, which until now was read once at startup
/// and so sat at whatever it was then — usually zero (`docs/12-decisions.md`).
pub(crate) fn cache_fetched(
    state: &mut AppState,
    track: ItemId,
    profile: crate::config::QualityProfile,
    total_bytes: u64,
) -> Vec<Effect> {
    state.cache_stats.audio_cache_bytes = total_bytes;
    if is_current_play(state, &track, profile) {
        state.player.playback_source = Some(crate::state::player::PlaybackSource::Cache);
    }
    state.touch();
    Vec::new()
}

fn is_current_play(
    state: &AppState,
    track: &ItemId,
    profile: crate::config::QualityProfile,
) -> bool {
    state.player.quality_profile == profile
        && state
            .queue
            .current()
            .is_some_and(|entry| entry.track.id == *track)
}

/// Whether a tab shows sectioned Artists/Albums/Tracks results rather than a Miller column. Both
/// such tabs reuse `SearchResults`/`SearchSection` wholesale, and `AppState::active_column` /
/// `AppState::selected_item` return `None` on either — so every queueing path has to read them
/// through [`focused_section_row`] instead of falling through to column logic.
fn is_sectioned_tab(tab: Tab) -> bool {
    matches!(tab, Tab::Search | Tab::Favourites)
}

/// The focused row of a sectioned tab, as `(results, section, cursor)`.
///
/// Only Search had this wiring. Favourites has the identical shape and had none of it, so `Enter`,
/// `a`, `A`, `i` and `m` all did nothing whatsoever on the tab — a live user reported it as "I
/// can't select anything from favourites" (`docs/12-decisions.md`). Routing both tabs through one
/// accessor is what makes them behave the same by construction rather than by duplication.
///
/// `None` for a tab with a real column, and for Search while the query line has focus (typing must
/// not queue).
fn focused_section_row(
    state: &AppState,
) -> Option<(&crate::state::search::SearchResults, SearchSection, usize)> {
    match state.nav.active_tab {
        Tab::Search => {
            if state.search.query_focused {
                return None;
            }
            let section = state.search.focused_section;
            Some((
                &state.search.results,
                section,
                state.search.cursors[section.index()],
            ))
        }
        Tab::Favourites => {
            let section = state.favourites.focused_section;
            Some((
                &state.favourites.results,
                section,
                state.favourites.cursors[section.index()],
            ))
        }
        _ => None,
    }
}

/// The focused row of a sectioned tab as a `MediaItem`, so container queueing can treat it exactly
/// like a column row. Cloned rather than borrowed: `SearchResults` stores `Artist`/`Album`/`Track`
/// directly, not `MediaItem`, so there is nothing of that type to hand back a reference to.
fn focused_section_item(state: &AppState) -> Option<MediaItem> {
    let (results, section, idx) = focused_section_row(state)?;
    match section {
        SearchSection::Artists => results.artists.get(idx).cloned().map(MediaItem::Artist),
        SearchSection::Albums => results.albums.get(idx).cloned().map(MediaItem::Album),
        SearchSection::Tracks => results.tracks.get(idx).cloned().map(MediaItem::Track),
        SearchSection::Playlists => results.playlists.get(idx).cloned().map(MediaItem::Playlist),
    }
}

/// Resolves the tracks to queue from whatever's currently selected — the focused row of a sectioned
/// tab, or `MediaItem::Track` rows in the active column. Container rows resolve to nothing *here*:
/// they need a network fetch, which [`container_fetch`] issues instead. Visual-multiselect keeps the
/// column's own display order, never the `BTreeSet`'s (alphabetical-by-id) order.
fn resolve_selection_tracks(state: &AppState) -> Vec<(Track, QueueSource)> {
    if is_sectioned_tab(state.nav.active_tab) {
        let source = if state.nav.active_tab == Tab::Search {
            QueueSource::Search
        } else {
            // No `QueueSource::Favourites` exists, and one would earn its keep only in the Now
            // Playing source badge. A favourite queued one row at a time is a hand-picked track,
            // which is exactly what `Manual` already means.
            QueueSource::Manual
        };
        return match focused_section_item(state) {
            Some(MediaItem::Track(t)) => vec![(t, source)],
            _ => Vec::new(),
        };
    }

    let Some(column) = state.active_column() else {
        return Vec::new();
    };
    let source = queue_source_for(&column.kind);

    let indices: Vec<usize> =
        if column.selection.visual_mode && !column.selection.selected.is_empty() {
            column
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.id()
                        .is_some_and(|id| column.selection.selected.contains(id))
                })
                .map(|(i, _)| i)
                .collect()
        } else {
            vec![column.cursor]
        };

    indices
        .into_iter()
        .filter_map(|i| column.items.get(i))
        .filter_map(|item| match item {
            MediaItem::Track(t) => Some((t.clone(), source.clone())),
            _ => None,
        })
        .collect()
}

fn queue_source_for(kind: &ColumnKind) -> QueueSource {
    match kind {
        ColumnKind::Tracks { of_album } => QueueSource::Album {
            id: of_album.clone(),
        },
        ColumnKind::ArtistTracks { of_artist } => QueueSource::Artist {
            id: of_artist.clone(),
        },
        ColumnKind::PlaylistTracks { of_playlist } => QueueSource::Playlist {
            id: PlaylistId::from(of_playlist.as_str()),
        },
        ColumnKind::SearchResults => QueueSource::Search,
        _ => QueueSource::Manual,
    }
}

fn queue_selection(state: &mut AppState, full_context: bool) -> Vec<Effect> {
    if let Some(effects) = queue_multi_selection(state, full_context, QueueBatchMode::Append) {
        return effects;
    }
    if let Some(effects) = queue_album_or_artist_row(state, full_context) {
        return effects;
    }
    let candidates = resolve_selection_tracks(state);
    append_tracks(state, candidates)
}

/// A visual multi-selection that contains at least one container row, resolved as a
/// [`QueueBatch`]: one fetch per container, plain tracks filled in immediately, everything
/// reassembled in display order once the last reply lands.
///
/// `v`-selecting albums and pressing `a` used to queue nothing — `resolve_selection_tracks` kept
/// only `MediaItem::Track` rows, so a selection of albums resolved to an empty candidate list and
/// silently did nothing (`docs/12-decisions.md`).
///
/// Returns `None` — leaving the caller's existing single-row and all-tracks paths untouched — when
/// there is no multi-selection, or when the selection is entirely tracks and so needs no fetch at
/// all. Rows that are neither tracks nor queueable containers (a section header, say) contribute no
/// slot rather than blocking the batch forever.
fn queue_multi_selection(
    state: &mut AppState,
    full_context: bool,
    mode: QueueBatchMode,
) -> Option<Vec<Effect>> {
    let column = state.active_column()?;
    if !column.selection.visual_mode || column.selection.selected.is_empty() {
        return None;
    }
    let source = queue_source_for(&column.kind);
    let selected: Vec<MediaItem> = column
        .items
        .iter()
        .filter(|item| {
            item.id()
                .is_some_and(|id| column.selection.selected.contains(id))
        })
        .cloned()
        .collect();
    if !selected
        .iter()
        .any(|item| !matches!(item, MediaItem::Track(_)))
    {
        return None;
    }

    let mut effects = Vec::new();
    let mut slots = Vec::new();
    for item in selected {
        match item {
            MediaItem::Track(track) => slots.push(QueueBatchSlot {
                awaiting: None,
                tracks: vec![(track, source.clone())],
            }),
            other => {
                if let Some((effect, awaiting)) = container_fetch(state, &other, full_context) {
                    effects.push(effect);
                    slots.push(QueueBatchSlot {
                        awaiting: Some(awaiting),
                        tracks: Vec::new(),
                    });
                }
            }
        }
    }
    start_queue_batch(state, QueueBatch { mode, slots }, effects)
}

/// Installs `batch` as the one in-flight batch and returns its fetch effects — or, when every slot
/// is already filled (nothing needed fetching), flushes it straight away rather than parking a
/// batch that no reply will ever complete.
fn start_queue_batch(
    state: &mut AppState,
    batch: QueueBatch,
    effects: Vec<Effect>,
) -> Option<Vec<Effect>> {
    if batch.slots.is_empty() {
        return Some(Vec::new());
    }
    let complete = batch.is_complete();
    state.pending_queue_batch = Some(batch);
    if complete {
        return Some(flush_queue_batch(state));
    }
    Some(effects)
}

/// Fills the slot awaiting `source` and, once no slot is still waiting, flushes the whole batch
/// into the queue in slot order.
fn fill_queue_batch_slot(
    state: &mut AppState,
    source: &QueueSource,
    candidates: Vec<(Track, QueueSource)>,
) -> Vec<Effect> {
    let Some(batch) = state.pending_queue_batch.as_mut() else {
        return Vec::new();
    };
    if let Some(slot) = batch
        .slots
        .iter_mut()
        .find(|slot| slot.awaiting.as_ref() == Some(source))
    {
        slot.awaiting = None;
        slot.tracks = candidates;
    }
    if !batch.is_complete() {
        return Vec::new();
    }
    flush_queue_batch(state)
}

fn flush_queue_batch(state: &mut AppState) -> Vec<Effect> {
    let Some(batch) = state.pending_queue_batch.take() else {
        return Vec::new();
    };
    let mode = batch.mode;
    let candidates = batch.into_tracks();
    match mode {
        QueueBatchMode::Append => append_tracks(state, candidates),
        QueueBatchMode::InsertNext => insert_tracks_next(state, candidates),
    }
}

/// A queue-directed fetch failed (`LoadTarget::QueueFetch`). The failure carries no source, so
/// there is no way to know *which* slot will never arrive — the batch is abandoned and whatever
/// already landed is queued, rather than leaving the user with a batch that can never complete and
/// therefore never queues anything at all. Replies that arrive afterwards are not lost: with no
/// batch to match, they take the ordinary append path.
pub(crate) fn queue_batch_fetch_failed(state: &mut AppState) -> Vec<Effect> {
    let Some(batch) = state.pending_queue_batch.as_ref() else {
        return Vec::new();
    };
    let abandoned: Vec<ItemId> = batch
        .slots
        .iter()
        .filter_map(|slot| match &slot.awaiting {
            Some(QueueSource::Album { id }) => Some(id.clone()),
            _ => None,
        })
        .collect();
    for id in abandoned {
        state.pending_album_queue_fetch.remove(&id);
    }
    flush_queue_batch(state)
}

/// `Enter` — replace the queue with the selection and play it now. Clears first, then runs the same
/// `queue_selection` path: for a track that appends into the now-empty queue and plays immediately;
/// for a container (album/artist/playlist/folder) that issues the same fetch, whose reply then fills
/// the empty queue and plays from the top. A container fetch that fails leaves the queue empty
/// (the old queue is genuinely gone) — the accepted cost of "replace" over "append"
/// (`docs/12-decisions.md`). Guarded so a selection that would queue *nothing* (e.g. a bare genre
/// row, or the query line on Search) does not needlessly wipe a queue that's already playing.
fn play_selection(state: &mut AppState, full_context: bool) -> Vec<Effect> {
    if !selection_is_queueable(state) {
        return queue_selection(state, full_context);
    }
    let mut effects = clear(state);
    effects.extend(queue_selection(state, full_context));
    effects
}

/// Whether the current selection would actually queue something — a container row, or a resolvable
/// track. Mirrors the two arms `queue_selection` itself dispatches to, so `play_selection` only
/// clears the queue when it's genuinely about to refill it.
fn selection_is_queueable(state: &AppState) -> bool {
    let focused = if is_sectioned_tab(state.nav.active_tab) {
        focused_section_item(state)
    } else {
        state.selected_item().cloned()
    };
    matches!(
        focused,
        Some(
            MediaItem::Album(_)
                | MediaItem::Artist(_)
                | MediaItem::Folder(_)
                | MediaItem::Playlist(_)
                | MediaItem::Genre(_)
        )
    ) || multi_selection_has_container(state)
        || !resolve_selection_tracks(state).is_empty()
}

/// Whether the active column's visual multi-selection holds at least one container row — the
/// condition under which `queue_selection` takes the batched path, so `play_selection` must count
/// it as "about to refill the queue" and clear first.
fn multi_selection_has_container(state: &AppState) -> bool {
    let Some(column) = state.active_column() else {
        return false;
    };
    column.selection.visual_mode
        && column.items.iter().any(|item| {
            !matches!(item, MediaItem::Track(_))
                && item
                    .id()
                    .is_some_and(|id| column.selection.selected.contains(id))
        })
}

/// `06-08`: the number of tracks requested from `/Items/{seed}/InstantMix` — this task's own spec
/// fixes it; it is not a user-configurable setting.
const INSTANT_MIX_LIMIT: usize = 100;

/// `m` — works for whatever's focused (artist, album, track, genre, playlist all accept an
/// `ItemId` seed to Emby's endpoint identically) and refuses outright while offline, since the
/// endpoint has no offline equivalent.
fn instant_mix(state: &mut AppState) -> Vec<Effect> {
    if state.connectivity == Connectivity::Offline {
        state.toast("instant mix requires a connection", ToastLevel::Warning);
        return Vec::new();
    }
    let Some((seed, name)) = resolve_instant_mix_seed(state) else {
        return Vec::new();
    };
    state.pending_instant_mix = Some(name);
    vec![Effect::Net(NetEffect::InstantMix {
        seed,
        limit: INSTANT_MIX_LIMIT,
    })]
}

/// The seed id + display name for an instant mix — the cursor item, or, in visual-select mode,
/// the *first* selected item in display order (never the `BTreeSet`'s own alphabetical-by-id
/// order), toasting that it did so the choice isn't mysterious to a user who selected several.
fn resolve_instant_mix_seed(state: &mut AppState) -> Option<(ItemId, String)> {
    if is_sectioned_tab(state.nav.active_tab) {
        let item = focused_section_item(state)?;
        return Some((item.id()?.clone(), item.display_name().to_string()));
    }
    let column = state.active_column()?;
    let is_multiselect = column.selection.visual_mode && !column.selection.selected.is_empty();
    let item = if is_multiselect {
        column.items.iter().find(|item| {
            item.id()
                .is_some_and(|id| column.selection.selected.contains(id))
        })?
    } else {
        column.items.get(column.cursor)?
    };
    let id = item.id()?.clone();
    let name = item.display_name().to_string();

    if is_multiselect {
        state.toast(format!("instant mix seeded from {name}"), ToastLevel::Info);
    }
    Some((id, name))
}

/// The reply to `Effect::Net(InstantMix)` — **replaces** the queue rather than appending (a mix is
/// a new listening session), badges every entry `QueueSource::InstantMix { seed }`, starts
/// playback at index 0, and deliberately does not apply the active sort profile (the server's own
/// ordering is the entire point of the feature). An empty reply is not an error — some libraries
/// genuinely cannot generate one — so the queue is left untouched rather than cleared with
/// nothing to replace it.
fn apply_instant_mix_reply(state: &mut AppState, tracks: Vec<Track>, seed: ItemId) -> Vec<Effect> {
    let name = state
        .pending_instant_mix
        .take()
        .unwrap_or_else(|| seed.to_string());

    if tracks.is_empty() {
        state.toast(
            format!("no instant mix available for {name}"),
            ToastLevel::Warning,
        );
        return Vec::new();
    }

    let mut entries = Vec::with_capacity(tracks.len());
    for track in tracks {
        let entry_id = state.queue.next_id();
        entries.push(QueueEntry {
            entry_id,
            track,
            source: QueueSource::InstantMix { seed: seed.clone() },
            availability: Availability::Remote,
        });
    }
    let count = entries.len();
    state.queue.entries = entries;
    state.queue.play_order = (0..count).collect();
    state.queue.position = 0;
    state.queue.shuffled = false;
    state.queue.mix_name = Some(name.clone());
    state.player.last_preloaded = None;
    state.toast(
        format!("instant mix: {count} tracks from {name}"),
        ToastLevel::Info,
    );
    state.touch();

    let effects = load_current(state);
    debug_assert_invariants(state);
    effects
}

/// The queue-directed fetch a container row needs, paired with the `QueueSource` its reply will
/// carry — the single place that maps a row kind to its fetch, so the single-row path
/// ([`queue_album_or_artist_row`]) and the batched path ([`queue_multi_selection`]) can never drift
/// apart on which rows are queueable or how.
///
/// `None` for a row that needs no fetch (a plain track, already in hand) or cannot be queued at all.
/// Takes `&mut AppState` because an `AppearsOn` album has to record its artist filter in
/// `pending_album_queue_fetch` — the fetch itself is identical either way.
fn container_fetch(
    state: &mut AppState,
    item: &MediaItem,
    full_context: bool,
) -> Option<(Effect, QueueSource)> {
    match item {
        MediaItem::Album(album) => {
            // Identical fetch either way (`album_row_a_and_shift_a_both_emit_identical_fetch_
            // effect`) — only the *filter to apply once the reply lands* depends on which key was
            // pressed, and that's remembered here, not in the effect.
            let filter_artist = match (&album.relation, full_context) {
                (AlbumRelation::AppearsOn { context_artist }, false) => {
                    Some(context_artist.clone())
                }
                _ => None,
            };
            state
                .pending_album_queue_fetch
                .insert(album.id.clone(), filter_artist);
            Some((
                Effect::Net(NetEffect::FetchAlbumTracksForQueue {
                    album: album.id.clone(),
                }),
                QueueSource::Album {
                    id: album.id.clone(),
                },
            ))
        }
        MediaItem::Artist(artist) => Some((
            Effect::Net(NetEffect::FetchArtistTracksForQueue {
                artist: artist.id.clone(),
            }),
            QueueSource::Artist {
                id: artist.id.clone(),
            },
        )),
        // A genre queues every track in it. It was refused outright on the grounds that a genre can
        // span tens of thousands of tracks — but a user selecting one is asking for exactly that,
        // and said so (`docs/12-decisions.md`). The append toast reports the count, so a genre
        // larger than expected is visible rather than silent.
        MediaItem::Genre(genre) => Some((
            Effect::Net(NetEffect::FetchGenreTracksForQueue {
                genre: genre.name.clone(),
            }),
            QueueSource::Genre {
                name: genre.name.clone(),
            },
        )),
        // A folder queues its **whole subtree**, always — "play this folder" means every track
        // under it, subfolders and all. This used to key off `full_context` (`a`/`Enter` shallow,
        // `A`/`Shift+Enter` recursive), but a live user found shallow-by-default surprising: a
        // parent/library folder holds only subfolders, so a non-recursive queue came back empty
        // ("Enter on a folder plays nothing"). They asked for it to just queue everything, so the
        // shallow variant is dropped (`docs/12-decisions.md`).
        MediaItem::Folder(f) => Some((
            Effect::Net(NetEffect::FetchFolderTracksForQueue {
                folder: f.id.clone(),
                recursive: true,
            }),
            QueueSource::Folder {
                id: f.id.clone(),
                recursive: true,
            },
        )),
        // A real gap found in the field: pressing `a`/`Enter` directly on a Playlist row (without
        // first drilling into its own tracks) used to fall all the way through to
        // `resolve_selection_tracks`, which has no `Track` to find on a bare `Playlist` item and
        // so silently queued nothing at all — "when i press enter on playlists it doesn't play"
        // (`docs/12-decisions.md`). Every other container row type already queues this way.
        MediaItem::Playlist(p) => Some((
            Effect::Net(NetEffect::FetchPlaylistTracksForQueue {
                playlist: PlaylistId::from(p.id.as_str()),
            }),
            QueueSource::Playlist {
                id: PlaylistId::from(p.id.as_str()),
            },
        )),
        _ => None,
    }
}

/// `06-02`: a single focused container row (not part of a multi-selection — that goes through
/// [`queue_multi_selection`] instead) needs a network fetch before anything can be queued. Returns
/// `None` for every other case, letting `queue_selection` fall back to its normal track-resolution
/// path.
fn queue_album_or_artist_row(state: &mut AppState, full_context: bool) -> Option<Vec<Effect>> {
    let item = focused_container_item(state)?;
    let (effect, _) = container_fetch(state, &item, full_context)?;
    Some(vec![effect])
}

/// The row a single-row container operation acts on: the focused result on a sectioned tab, or the
/// cursor row of the active column — and, on a column, only when a visual multi-selection is *not*
/// in force, since that case is the batched path's.
fn focused_container_item(state: &AppState) -> Option<MediaItem> {
    if is_sectioned_tab(state.nav.active_tab) {
        return focused_section_item(state);
    }
    let column = state.active_column()?;
    if column.selection.visual_mode && !column.selection.selected.is_empty() {
        return None;
    }
    column.items.get(column.cursor).cloned()
}

/// `DataAction::TracksLoaded` — the reply to a queue-directed fetch
/// (`FetchAlbumTracksForQueue`/`FetchArtistTracksForQueue`). A Tracks/PlaylistTracks/etc. *column*
/// population reply is `ItemsLoaded`, handled entirely in `reducer::nav` and never routed here.
pub fn apply_tracks_loaded(
    state: &mut AppState,
    tracks: Vec<Track>,
    source: QueueSource,
    _full_context: bool,
) -> Vec<Effect> {
    // `06-08`: an instant-mix reply is a queue *replacement*, not an appends-to/filters-existing
    // flow — routed to its own function entirely rather than shoehorned into the branches below.
    if let QueueSource::InstantMix { seed } = &source {
        return apply_instant_mix_reply(state, tracks, seed.clone());
    }

    let filter = match &source {
        QueueSource::Album { id } => state
            .pending_album_queue_fetch
            .remove(id)
            .flatten()
            .map(TrackFilter::ArtistOnly)
            .unwrap_or(TrackFilter::All),
        _ => TrackFilter::All,
    };

    let mut filtered = filter_tracks(&tracks, &filter);

    // Folder queueing always toasts the count, including zero — a folder now queues its whole
    // subtree, and the count is what makes "how much did that just pull in?" (and "that folder was
    // empty") discoverable either way.
    if let QueueSource::Folder { .. } = &source {
        state.toast(
            format!(
                "queued {} track{} from this folder",
                filtered.len(),
                if filtered.len() == 1 { "" } else { "s" }
            ),
            ToastLevel::Info,
        );
    }

    // A batched reply must fill its slot even when it resolves to nothing, or the batch waits for
    // it forever. The warnings below still fire — they say something true about *this* container —
    // but they must not short-circuit the slot fill.
    let batched = state
        .pending_queue_batch
        .as_ref()
        .is_some_and(|batch| batch.awaits(&source));

    if filtered.is_empty() {
        if let TrackFilter::ArtistOnly(artist) = &filter {
            // No artist-name lookup exists at this layer for an arbitrary `context_artist` id
            // (`AlbumRelation::AppearsOn` carries only the id, and there is no artist-directory
            // cache on `AppState` to resolve it from) — the id is what's shown until one exists.
            // See `docs/12-decisions.md`.
            state.toast(
                format!("no tracks by {artist} on this release"),
                ToastLevel::Warning,
            );
        } else if matches!(source, QueueSource::Artist { .. }) {
            // An artist that resolves to no tracks at all (even after `artist_tracks`' own
            // `AlbumArtistIds` fallback) must say so rather than silently doing nothing — a live
            // user read that silence as "Enter is broken on artists" (`docs/12-decisions.md`).
            state.toast("no playable tracks for this artist", ToastLevel::Warning);
        }
        if !batched {
            return Vec::new();
        }
    }

    order_incoming(state, &source, &mut filtered);

    let candidates: Vec<(Track, QueueSource)> =
        filtered.into_iter().map(|t| (t, source.clone())).collect();
    if batched {
        return fill_queue_batch_slot(state, &source, candidates);
    }
    append_tracks(state, candidates)
}

/// Orders freshly fetched tracks before they join the queue.
///
/// Three cases, because "the right order" genuinely differs by what was selected:
///
/// * A **folder** keeps the order the server gave it — file path, i.e. the directory structure —
///   which is the whole point of queueing a folder (`docs/12-decisions.md`).
/// * A source spanning **many albums** (a genre, an artist) uses the user's own queue sort profile.
///   These used to be sorted by `(disc, track)` like an album, which across albums is nonsense: it
///   groups every album's track 1 together, then every track 2. A user asked for genres to follow
///   the configured order, and artists had the identical defect.
/// * Anything else — an album, a playlist, a hand-picked selection — keeps album order, disc then
///   track. Pressing `Enter` on an album means "play this album", not "apply my library sort to
///   it", so the profile deliberately does *not* reach here.
///
/// Uses `compare_with_album_year`, not a bare `compare`, so a `Year` rule keys off each album's own
/// year rather than each track's — the same correction `apply_sort_profile` makes, for the same
/// reason (`docs/12-decisions.md`).
fn order_incoming(state: &AppState, source: &QueueSource, tracks: &mut [Track]) {
    match source {
        QueueSource::Folder { .. } => {}
        QueueSource::Genre { .. } | QueueSource::Artist { .. } => {
            match active_queue_profile(state) {
                Some(profile) => {
                    let years = sort::album_years(tracks.iter());
                    tracks.sort_by(|a, b| sort::compare_with_album_year(a, b, &profile, &years));
                }
                // No profile configured (or it names one that no longer exists): album order is a
                // better default than the order the server happened to return.
                None => sort_by_album_order(tracks),
            }
        }
        _ => sort_by_album_order(tracks),
    }
}

fn sort_by_album_order(tracks: &mut [Track]) {
    tracks.sort_by_key(|t| (t.disc_number.unwrap_or(0), t.track_number.unwrap_or(0)));
}

/// The profile new tracks should be ordered by: whichever the queue is currently sorted under, else
/// the configured default. Preferring the queue's own means tracks appended into a queue the user
/// has just re-sorted arrive in that same order rather than reverting to the config's.
fn active_queue_profile(state: &AppState) -> Option<crate::config::SortProfile> {
    let name = state
        .queue
        .sort_profile
        .as_deref()
        .unwrap_or(state.config.sorting.default_queue_profile.as_str());
    state
        .config
        .sorting
        .profiles
        .iter()
        .find(|p| p.name == name)
        .cloned()
}

/// Production entry point: computes `Availability` per current connectivity (no real cache
/// manifest exists yet — see this module's own doc comment) and gates/appends.
fn append_tracks(state: &mut AppState, candidates: Vec<(Track, QueueSource)>) -> Vec<Effect> {
    let connectivity = state.connectivity;
    let availability = if connectivity == Connectivity::Offline {
        Availability::Unavailable
    } else {
        Availability::Remote
    };
    let with_availability = candidates
        .into_iter()
        .map(|(t, s)| (t, s, availability))
        .collect();
    append_tracks_gated(state, with_availability)
}

/// The actual gate + append logic, taking pre-computed `Availability` per candidate — factored out
/// so `06-02`'s eventual network-fetched candidates (and this task's own availability-gating
/// tests) can drive it directly without needing a real cache manifest to exist first.
fn append_tracks_gated(
    state: &mut AppState,
    candidates: Vec<(Track, QueueSource, Availability)>,
) -> Vec<Effect> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let connectivity = state.connectivity;
    let total = candidates.len();
    let was_empty = state.queue.entries.is_empty();
    // Captured before the append: `restored_unloaded` is cleared below, and both are needed
    // afterwards to tell "this is about to start playing" from "this quietly grew the queue".
    let will_play = was_empty || state.player.restored_unloaded;
    // Folders announce their own count before reaching here, so they must not be told twice.
    let from_folder = candidates
        .iter()
        .any(|(_, source, _)| matches!(source, QueueSource::Folder { .. }));
    let mut queued = 0usize;

    for (track, source, availability) in candidates {
        if !is_queueable(availability, connectivity) {
            continue;
        }
        let entry_id = state.queue.next_id();
        state.queue.entries.push(QueueEntry {
            entry_id,
            track,
            source,
            availability,
        });
        let new_index = state.queue.entries.len() - 1;
        state.queue.play_order.push(new_index);
        queued += 1;
    }

    let skipped = total - queued;
    if queued == 0 && skipped > 0 {
        state.toast("not available offline", ToastLevel::Warning);
    } else if skipped > 0 {
        state.toast(
            format!("queued {queued}, skipped {skipped} (offline)"),
            ToastLevel::Warning,
        );
    } else if queued > 0 && !will_play && !from_folder {
        // An append that isn't about to start playing changes nothing you can see: the queue lives
        // on another tab, so "Add to Queue" read as a no-op even when it had worked
        // (`docs/12-decisions.md`). The play path stays quiet — playback starting is its own
        // feedback.
        state.toast(
            format!(
                "added {queued} track{} to the queue",
                if queued == 1 { "" } else { "s" }
            ),
            ToastLevel::Info,
        );
    }

    state.touch();
    // A restored-but-paused session (`session::restore`, `restored_unloaded`) leaves
    // `queue.entries` non-empty with nothing actually loaded into the audio engine — `was_empty`
    // alone can't see that, so newly queued tracks would just sit appended behind yesterday's
    // session forever, and a user who never happens to press Space (the other path that clears
    // `restored_unloaded`) would see every subsequent selection silently do nothing (a real bug
    // report, `docs/12-decisions.md`). Jumps straight to the first newly queued track rather than
    // replaying the stale restored one, matching what selecting something new is actually asking
    // for; the `was_empty` case is unaffected (`play_order.len() - queued == 0` there, same as
    // before).
    let effects = if will_play && queued > 0 {
        state.queue.position = state.queue.play_order.len() - queued;
        state.player.restored_unloaded = false;
        load_current(state)
    } else {
        Vec::new()
    };
    debug_assert_invariants(state);
    effects
}

/// `docs/12-decisions.md`: the only rule the gate currently expresses is "genuinely `Unavailable`
/// while offline" — every freshly-resolved track defaults to exactly that combination (see
/// `append_tracks`) absent a real cache manifest, but the gate itself is written against
/// `Availability` directly so `Cached`/`Downloaded` entries (however they come to exist) already
/// pass correctly, unchanged, once phase 08 gives `append_tracks` real per-track values to compute.
fn is_queueable(availability: Availability, connectivity: Connectivity) -> bool {
    !(availability == Availability::Unavailable && connectivity == Connectivity::Offline)
}

/// `06-06`'s own, *stricter* gate for preloading specifically — deliberately not [`is_queueable`]:
/// queueing an `Unavailable` track for *later* (once connectivity returns) is fine, but preloading
/// means buffering it *right now*, which an `Unavailable` entry can never satisfy regardless of
/// connectivity, and a `Remote` entry can't satisfy while offline either (it would need a network
/// fetch this instant). `Cached`/`Downloaded` entries are always preloadable, online or off.
fn can_preload(availability: Availability, connectivity: Connectivity) -> bool {
    if availability == Availability::Unavailable {
        return false;
    }
    !(availability == Availability::Remote && connectivity == Connectivity::Offline)
}

/// `i` — put the selection immediately after the current track.
///
/// A container row needs its tracks fetched first, and the reply carries no record of which key
/// asked for it, so `i` on an album or artist used to fall through to `resolve_selection_tracks`,
/// find no `Track` row, and do nothing at all — "`i` inserts tracks, not albums or artists"
/// (`docs/12-decisions.md`). A single-slot [`QueueBatch`] is what carries the insert-vs-append
/// intent across the round trip; a multi-selection uses the same machinery with more slots.
fn insert_next(state: &mut AppState) -> Vec<Effect> {
    if let Some(effects) = queue_multi_selection(state, false, QueueBatchMode::InsertNext) {
        return effects;
    }
    if let Some(item) = focused_container_item(state)
        && let Some((effect, awaiting)) = container_fetch(state, &item, false)
    {
        return start_queue_batch(
            state,
            QueueBatch {
                mode: QueueBatchMode::InsertNext,
                slots: vec![QueueBatchSlot {
                    awaiting: Some(awaiting),
                    tracks: Vec::new(),
                }],
            },
            vec![effect],
        )
        .unwrap_or_default();
    }
    insert_tracks_next(state, resolve_selection_tracks(state))
}

fn insert_tracks_next(state: &mut AppState, candidates: Vec<(Track, QueueSource)>) -> Vec<Effect> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let connectivity = state.connectivity;
    let availability = if connectivity == Connectivity::Offline {
        Availability::Unavailable
    } else {
        Availability::Remote
    };

    let was_empty = state.queue.entries.is_empty();
    let mut new_indices = Vec::new();
    for (track, source) in candidates {
        if !is_queueable(availability, connectivity) {
            continue;
        }
        let entry_id = state.queue.next_id();
        state.queue.entries.push(QueueEntry {
            entry_id,
            track,
            source,
            availability,
        });
        new_indices.push(state.queue.entries.len() - 1);
    }
    if new_indices.is_empty() {
        state.toast("not available offline", ToastLevel::Warning);
        return Vec::new();
    }

    let insert_at = if state.queue.play_order.is_empty() {
        0
    } else {
        state.queue.position + 1
    };
    for (offset, index) in new_indices.into_iter().enumerate() {
        state.queue.play_order.insert(insert_at + offset, index);
    }

    state.touch();
    // Same restored-but-unloaded gap as `append_tracks_gated` above, and the same fix: also start
    // playback when nothing has actually been loaded into the engine yet, landing on the
    // newly-inserted track (`insert_at`) rather than the stale restored one.
    let effects = if was_empty || state.player.restored_unloaded {
        state.queue.position = insert_at;
        state.player.restored_unloaded = false;
        load_current(state)
    } else {
        Vec::new()
    };
    debug_assert_invariants(state);
    effects
}

/// The three invariants `06-01`'s own spec requires after every mutation. `debug_assert!` only —
/// a release build must never crash over a state a user can't do anything about; a bug here is
/// this reducer's own fault, and the panic-free rule (`docs/04-state-and-input.md` §4 rule 1)
/// still holds in release.
fn debug_assert_invariants(state: &AppState) {
    #[cfg(debug_assertions)]
    {
        let q = &state.queue;
        let mut sorted = q.play_order.clone();
        sorted.sort_unstable();
        debug_assert_eq!(
            sorted,
            (0..q.entries.len()).collect::<Vec<_>>(),
            "play_order must be a permutation of 0..entries.len()"
        );
        debug_assert!(
            q.play_order.is_empty() || q.position < q.play_order.len(),
            "position must be within play_order when the queue is non-empty"
        );
        let mut ids: Vec<_> = q.entries.iter().map(|e| e.entry_id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        debug_assert_eq!(ids.len(), before, "entry_ids must be unique");
    }
    let _ = state;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, DataAction, ModalAction, QueueAction};
    use crate::model::PlaybackReport;
    use crate::reducer;
    use crate::state::nav::{Column, NavFocus, Tab};
    use crate::test_support::fixtures;
    use proptest::prelude::*;

    fn focus_column(mut state: AppState, tab: Tab, depth: usize) -> AppState {
        state.nav.active_tab = tab;
        state.nav.focus = NavFocus::Column(depth);
        state
    }

    /// `fixture_miller_3col()`'s Tracks column (depth 2): 3 tracks, cursor already on index 1
    /// ("Fate"), no visual selection.
    fn tracks_state() -> AppState {
        focus_column(fixtures::fixture_miller_3col(), Tab::Artists, 2)
    }

    fn dispatch(state: &mut AppState, action: Action) -> Vec<Effect> {
        reducer::apply(state, action)
    }

    fn track_ids(state: &AppState) -> Vec<ItemId> {
        state
            .active_column()
            .unwrap()
            .items
            .iter()
            .filter_map(MediaItem::id)
            .cloned()
            .collect()
    }

    #[test]
    fn play_selection_replaces_the_existing_queue_and_plays() {
        // A queue that's already playing three tracks; `Enter` (PlaySelection) on a different track
        // must wipe it and play the new selection, not append (the user's expectation).
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        state.active_column_mut().unwrap().cursor = 2;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 2, "two tracks appended so far");

        // Now Enter on the first track: replace.
        state.active_column_mut().unwrap().cursor = 0;
        let selected = match &state.active_column().unwrap().items[0] {
            MediaItem::Track(t) => t.id.clone(),
            other => panic!("expected a track, got {other:?}"),
        };
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::PlaySelection {
                full_context: false,
            }),
        );
        assert_eq!(
            state.queue.entries.len(),
            1,
            "the queue was replaced, not appended to"
        );
        assert_eq!(state.queue.entries[0].track.id, selected);
        assert_eq!(state.queue.position, 0);
        assert_eq!(state.player.current, Some(state.queue.entries[0].entry_id));
        // The replacement stops the old track (clear) and loads the new one.
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Stop))),
            "replacing stops the previous track"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Load { .. }))),
            "and loads the new one"
        );
    }

    #[test]
    fn play_selection_on_a_non_queueable_row_does_not_wipe_the_queue() {
        // A section header queues nothing — `Enter` on it must not clear a queue that's happily
        // playing. (Genres *are* queueable now, so they no longer serve as the example here.)
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 1);

        // Swap the focused column for a section header (nothing queueable).
        let header = MediaItem::SectionHeader(crate::model::SectionHeader {
            label: "ALBUMS".to_string(),
            count: 0,
            kind: crate::model::SectionKind::Albums,
        });
        state.active_column_mut().unwrap().items = vec![header];
        state.active_column_mut().unwrap().cursor = 0;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::PlaySelection {
                full_context: false,
            }),
        );
        assert_eq!(
            state.queue.entries.len(),
            1,
            "a non-queueable selection must leave the queue intact"
        );
    }

    #[test]
    fn append_to_empty_queue_starts_playback() {
        let mut state = tracks_state();
        assert!(state.queue.entries.is_empty());
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 1);
        assert_eq!(state.queue.position, 0);
        // `08-03`: `load_current` now always emits `EnsureCached` immediately before `Load`.
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(_),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(SysEffect::Notify(_)),
                Effect::Sys(SysEffect::UpdateMpris(_))
            ]
        ));
        assert_eq!(state.player.current, Some(state.queue.entries[0].entry_id));
    }

    /// Adding to a queue that is already playing changes nothing on screen — the queue lives on
    /// another tab — so the action read as a no-op even when it had worked (`docs/12-decisions.md`).
    #[test]
    fn appending_to_a_playing_queue_says_so() {
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        state.toasts.clear();
        state.active_column_mut().unwrap().cursor = 2;

        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );

        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("added 1 track to the queue")),
            "an otherwise invisible append must announce itself: {:?}",
            state.toasts
        );
    }

    /// Starting playback is its own feedback, so the first selection into an empty queue stays
    /// quiet rather than toasting over the track that just began.
    #[test]
    fn filling_an_empty_queue_and_playing_stays_quiet() {
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            !state.toasts.iter().any(|t| t.message.contains("added")),
            "playback starting speaks for itself: {:?}",
            state.toasts
        );
    }

    #[test]
    fn append_to_playing_queue_does_not_interrupt() {
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let position_before = state.queue.position;
        let current_before = state.player.current;

        // Move the cursor to a different track and queue again.
        state.active_column_mut().unwrap().cursor = 2;
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 2);
        assert_eq!(state.queue.position, position_before);
        assert_eq!(state.player.current, current_before);
        // `06-06`: appending created a next entry where there wasn't one before — that's
        // preloaded, but playback itself must still not be interrupted (no `Load`).
        assert!(
            matches!(
                effects.as_slice(),
                [
                    Effect::Audio(AudioEffect::Preload { .. }),
                    Effect::Cache(prefetch)
                ] if matches!(**prefetch, CacheEffect::PrefetchAhead { .. })
            ),
            "appending to a non-empty queue must not re-trigger Load"
        );
    }

    /// `cache_resolved` is the only place that learns whether a track plays from disk or the
    /// network, so it is what the player bar's readout keys off.
    #[test]
    fn cache_resolved_records_where_the_track_is_playing_from() {
        use crate::state::player::PlaybackSource;

        let mut state = fixtures::fixture_playing_queue();
        let track = state.current_entry().unwrap().track.id.clone();
        let profile = state.player.quality_profile;

        super::cache_resolved(
            &mut state,
            track.clone(),
            profile,
            Some(std::path::PathBuf::from("/cache/tracks/srv/x.direct.mp3")),
            crate::effect::RedactedUrl::new("http://example/stream".to_string()),
        );
        assert_eq!(state.player.playback_source, Some(PlaybackSource::Cache));

        super::cache_resolved(
            &mut state,
            track,
            profile,
            None,
            crate::effect::RedactedUrl::new("http://example/stream".to_string()),
        );
        assert_eq!(
            state.player.playback_source,
            Some(PlaybackSource::Streaming)
        );
    }

    /// A new track's source is unknown until its own lookup answers — carrying the previous
    /// track's answer over would assert something never checked.
    /// "Force cache is enabled but the source readout still shows stream." It always did: the
    /// source was decided once, at resolve time, and a fetch that completed mid-track told nobody
    /// (`docs/12-decisions.md`).
    #[test]
    fn the_source_readout_follows_the_cache_fetch() {
        use crate::state::player::PlaybackSource;
        let mut state = fixtures::fixture_playing_queue();
        let track = state.queue.current().unwrap().track.id.clone();
        let profile = state.player.quality_profile;
        state.player.playback_source = Some(PlaybackSource::Streaming);

        dispatch(
            &mut state,
            Action::Data(DataAction::CacheFetchStarted {
                track: track.clone(),
                profile,
            }),
        );
        assert_eq!(
            state.player.playback_source,
            Some(PlaybackSource::Caching),
            "a user who turned caching on must be able to see it happening"
        );

        dispatch(
            &mut state,
            Action::Data(DataAction::CacheFetched {
                track,
                profile,
                total_bytes: 4_096,
            }),
        );
        assert_eq!(state.player.playback_source, Some(PlaybackSource::Cache));
        assert_eq!(
            state.cache_stats.audio_cache_bytes, 4_096,
            "the About view's cache size was read once at startup and never moved again"
        );
    }

    /// A fetch finishing for some *other* track must not relabel what is playing.
    #[test]
    fn another_tracks_fetch_does_not_touch_the_readout() {
        use crate::state::player::PlaybackSource;
        let mut state = fixtures::fixture_playing_queue();
        let profile = state.player.quality_profile;
        state.player.playback_source = Some(PlaybackSource::Streaming);

        dispatch(
            &mut state,
            Action::Data(DataAction::CacheFetched {
                track: ItemId::from("some-other-track"),
                profile,
                total_bytes: 99,
            }),
        );
        assert_eq!(
            state.player.playback_source,
            Some(PlaybackSource::Streaming)
        );
        assert_eq!(
            state.cache_stats.audio_cache_bytes, 99,
            "the total is the cache's, not this track's — it still updates"
        );
    }

    #[test]
    fn loading_a_new_track_forgets_the_previous_source() {
        use crate::state::player::PlaybackSource;

        let mut state = fixtures::fixture_playing_queue();
        state.player.playback_source = Some(PlaybackSource::Cache);

        dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Next),
        );

        assert_eq!(state.player.playback_source, None);
    }

    fn state_with_custom_headers() -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.config.active_server = "srv-1".to_string();
        state.config.servers = vec![crate::config::ServerConfig {
            id: "srv-1".to_string(),
            name: "Behind a proxy".to_string(),
            url: "https://example.invalid".to_string(),
            user_id: "u".to_string(),
            access_token: "t".to_string(),
            device_id: "d".to_string(),
            custom_headers: [("CF-Access-Client-Id".to_string(), "id-value".to_string())]
                .into_iter()
                .collect(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }];
        state
    }

    fn load_headers(effects: &[Effect]) -> Option<std::collections::BTreeMap<String, String>> {
        effects.iter().find_map(|e| match e {
            Effect::Audio(AudioEffect::Load { headers, .. }) => Some(headers.clone()),
            _ => None,
        })
    }

    /// mpv issues its **own** HTTP request and does not share `EmbyClient`'s header map, so a
    /// server behind an authenticating proxy needs the custom headers repeated on the Load. They
    /// were never attached: browsing worked (that goes through `reqwest`) while every track failed
    /// with `Failed to recognize file format`, mpv having been handed the proxy's HTML deny page
    /// (`docs/12-decisions.md`).
    #[test]
    fn the_corrective_load_carries_the_servers_custom_headers() {
        let mut state = state_with_custom_headers();
        let track = state.current_entry().unwrap().track.id.clone();
        let profile = state.player.quality_profile;

        let effects = super::cache_resolved(
            &mut state,
            track,
            profile,
            None, // no local copy: this is the network-stream case
            crate::effect::RedactedUrl::new(
                "https://example.invalid/emby/Audio/1/stream".to_string(),
            ),
        );

        let headers = load_headers(&effects).expect("a Load effect");
        assert_eq!(
            headers.get("CF-Access-Client-Id").map(String::as_str),
            Some("id-value"),
            "mpv must be given the same headers the API client uses"
        );
    }

    /// A cached file is opened from disk, where headers mean nothing — and attaching credentials to
    /// a local path is pointless at best.
    #[test]
    fn a_local_file_load_carries_no_headers() {
        let mut state = state_with_custom_headers();
        let track = state.current_entry().unwrap().track.id.clone();
        let profile = state.player.quality_profile;

        let effects = super::cache_resolved(
            &mut state,
            track,
            profile,
            Some(std::path::PathBuf::from("/cache/tracks/srv/x.direct.flac")),
            crate::effect::RedactedUrl::new(
                "https://example.invalid/emby/Audio/1/stream".to_string(),
            ),
        );

        assert!(load_headers(&effects).expect("a Load effect").is_empty());
    }

    /// A server with no custom headers configured must not gain any.
    #[test]
    fn a_plain_server_sends_no_headers() {
        let mut state = fixtures::fixture_playing_queue();
        let track = state.current_entry().unwrap().track.id.clone();
        let profile = state.player.quality_profile;

        let effects = super::cache_resolved(
            &mut state,
            track,
            profile,
            None,
            crate::effect::RedactedUrl::new(
                "https://example.invalid/emby/Audio/1/stream".to_string(),
            ),
        );

        assert!(load_headers(&effects).expect("a Load effect").is_empty());
    }

    #[test]
    fn cache_resolved_with_no_local_copy_issues_load_at_the_stream_url() {
        // Real bug: `load_current` only ever starts playback with `placeholder_url`'s inert
        // `emby-track:{id}` scheme, which mpv cannot open — a track with no local cache copy (i.e.
        // almost any track never played before) never actually played at all, because
        // `cache_resolved`'s `path: None` case used to be a silent no-op. Confirms the fix: it now
        // issues a real corrective `Load` at the given `stream_url`.
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let track_id = state.queue.entries[0].track.id.clone();
        let profile = state.player.quality_profile;

        let effects = dispatch(
            &mut state,
            Action::Data(DataAction::CacheResolved {
                track: track_id,
                profile,
                path: None,
                stream_url: crate::effect::RedactedUrl::new(
                    "http://server/Audio/t1/stream?api_key=tok".to_string(),
                ),
            }),
        );

        assert!(matches!(
            effects.as_slice(),
            [Effect::Audio(AudioEffect::Load { url, .. })]
                if url.as_str() == "http://server/Audio/t1/stream?api_key=tok"
        ));
    }

    #[test]
    fn cache_resolved_is_discarded_when_stale() {
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let profile = state.player.quality_profile;

        // A reply for a track that is no longer current (the user has since skipped away) must
        // change nothing.
        let effects = dispatch(
            &mut state,
            Action::Data(DataAction::CacheResolved {
                track: ItemId::from("some-other-track"),
                profile,
                path: None,
                stream_url: crate::effect::RedactedUrl::new("http://server/x".to_string()),
            }),
        );
        assert!(effects.is_empty());
    }

    #[test]
    fn queueing_a_new_track_starts_playback_when_restored_but_never_loaded() {
        // Real bug: a restored-but-paused session (`session::restore`) leaves `queue.entries`
        // non-empty with nothing actually loaded into the audio engine (`restored_unloaded`) —
        // `was_empty` alone couldn't see that, so a user browsing to something new and pressing
        // Enter saw it silently appended behind yesterday's queue forever, with playback never
        // starting unless they happened to press Space first (`docs/12-decisions.md`).
        let mut state = tracks_state();
        let restored_track = fixtures::track(
            "Yesterday's Song",
            1,
            &fixtures::album("Old Album", 2019, &fixtures::artist("Old Artist")),
            &[&fixtures::artist("Old Artist")],
        );
        let restored_id = state.queue.next_id();
        state.queue.entries.push(crate::state::queue::QueueEntry {
            entry_id: restored_id,
            track: restored_track,
            source: crate::state::queue::QueueSource::Manual,
            availability: crate::state::queue::Availability::Remote,
        });
        state.queue.play_order.push(0);
        state.queue.position = 0;
        state.player.current = Some(restored_id);
        state.player.restored_unloaded = true;

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );

        assert!(!state.player.restored_unloaded);
        assert_eq!(state.queue.entries.len(), 2, "the new track was appended");
        assert_eq!(
            state.queue.position, 1,
            "position jumped to the newly queued track, not the stale restored one"
        );
        assert_ne!(
            state.player.current,
            Some(restored_id),
            "the restored track must not be what actually starts playing"
        );
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(_),
                Effect::Audio(AudioEffect::Load { .. }),
                ..
            ]
        ));
    }

    /// A restored session sets `player.current` without going through `load_current`, so nothing
    /// ever requested lyrics for the restored track and the pane sat on "loading lyrics…" forever
    /// with no request in the logs to explain it (`docs/12-decisions.md`).
    #[test]
    fn a_restored_track_still_fetches_its_lyrics() {
        let mut state = tracks_state();
        state.config.ui.show_lyrics = true;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        // Give the queued entry a lyric stream, as a real tagged track would have.
        state.queue.entries[0].track.lyric_stream = Some(crate::model::LyricStreamRef {
            media_source_id: crate::model::MediaSourceId::from("src-1"),
            stream_index: 1,
            format: crate::model::LyricFormat::Lrc,
        });
        state.lyrics = None;

        let effects = resume_after_restore(&mut state);

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Net(NetEffect::FetchLyrics { .. }))),
            "resuming a restored track must request its lyrics: {effects:?}"
        );
    }

    #[test]
    fn duplicate_track_gets_distinct_entry_ids() {
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 2);
        assert_ne!(
            state.queue.entries[0].entry_id,
            state.queue.entries[1].entry_id
        );
        assert_eq!(
            state.queue.entries[0].track.id,
            state.queue.entries[1].track.id
        );
    }

    #[test]
    fn insert_next_places_after_current() {
        let mut state = tracks_state();
        // Queue track 0, then track 1 (now playing track 0, one entry after it).
        state.active_column_mut().unwrap().cursor = 0;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        state.active_column_mut().unwrap().cursor = 1;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        // Now insert-next track 2: it must land immediately after position (index 0), before the
        // previously-appended track 1.
        state.active_column_mut().unwrap().cursor = 2;
        dispatch(&mut state, Action::Queue(QueueAction::InsertNext));

        assert_eq!(state.queue.play_order.len(), 3);
        let ids = track_ids(&state);
        let entry_at = |play_pos: usize| {
            state.queue.entries[state.queue.play_order[play_pos]]
                .track
                .id
                .clone()
        };
        assert_eq!(entry_at(0), ids[0]);
        assert_eq!(
            entry_at(1),
            ids[2],
            "InsertNext must land immediately after position"
        );
        assert_eq!(entry_at(2), ids[1]);
    }

    #[test]
    fn remove_current_entry_advances() {
        let mut state = tracks_state();
        for i in 0..3 {
            state.active_column_mut().unwrap().cursor = i;
            dispatch(
                &mut state,
                Action::Queue(QueueAction::QueueSelection {
                    full_context: false,
                }),
            );
        }
        assert_eq!(state.queue.position, 0);
        let second_id = state.queue.entries[state.queue.play_order[1]].entry_id;

        let effects = dispatch(&mut state, Action::Queue(QueueAction::RemoveEntry));
        assert_eq!(state.queue.entries.len(), 2);
        // The removed (first) entry is gone; position 0 now holds what was previously second.
        assert_eq!(state.queue.position, 0);
        assert_eq!(
            state.queue.entries[state.queue.play_order[0]].entry_id,
            second_id
        );
        // `06-06`: removing the current entry both reloads (the promoted entry) and preloads
        // (the one now after it). `08-03`: `load_current` also emits `EnsureCached` before `Load`.
        // `10-10`: and its own trailing notify effect, before the `Preload` that follows the whole
        // `load_current` call. The leading `Stopped` closes out the play being interrupted — it
        // has to come first, while the outgoing entry is still in `queue.entries` to be resolved.
        assert!(
            matches!(
                effects.as_slice(),
                [
                    Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Stopped { .. })),
                    Effect::Cache(_),
                    Effect::Audio(AudioEffect::Load { .. }),
                    Effect::Sys(SysEffect::Notify(_)),
                    Effect::Sys(SysEffect::UpdateMpris(_)),
                    Effect::Audio(AudioEffect::Preload { .. }),
                    Effect::Cache(prefetch)
                ] if matches!(**prefetch, CacheEffect::PrefetchAhead { .. })
            ),
            "got {effects:?}"
        );
    }

    /// Removing the last entry updated only the mirror — `current = None`, status `Stopped` — and
    /// emitted nothing, so the player bar said "nothing playing" while mpv carried on playing the
    /// track (`docs/12-decisions.md`).
    #[test]
    fn removing_the_last_entry_actually_stops_the_engine() {
        let mut state = tracks_state();
        state.active_column_mut().unwrap().cursor = 0;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 1);

        let effects = dispatch(&mut state, Action::Queue(QueueAction::RemoveEntry));
        assert!(state.queue.entries.is_empty());
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Audio(AudioEffect::Stop))),
            "the engine must be told to stop, got {effects:?}"
        );
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::ReportPlayback(PlaybackReport::Stopped { .. }))
            )),
            "the play must be closed out with the server too, got {effects:?}"
        );
        assert_eq!(state.player.status, PlayStatus::Stopped);
        assert!(state.player.current.is_none());
        assert_eq!(state.player.position, std::time::Duration::ZERO);
        assert!(
            !state.player.restored_unloaded,
            "nothing is left to resume, so this must not look like a restored session"
        );
    }

    #[test]
    fn remove_renumbers_play_order() {
        let mut state = tracks_state();
        for i in 0..3 {
            state.active_column_mut().unwrap().cursor = i;
            dispatch(
                &mut state,
                Action::Queue(QueueAction::QueueSelection {
                    full_context: false,
                }),
            );
        }
        // Remove the *current* (first) entry; the remaining two entries' indices must be
        // renumbered so `play_order` stays a valid permutation of 0..len().
        dispatch(&mut state, Action::Queue(QueueAction::RemoveEntry));
        let mut sorted = state.queue.play_order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1]);
    }

    #[test]
    fn move_entry_does_not_reorder_entries() {
        let mut state = tracks_state();
        for i in 0..3 {
            state.active_column_mut().unwrap().cursor = i;
            dispatch(
                &mut state,
                Action::Queue(QueueAction::QueueSelection {
                    full_context: false,
                }),
            );
        }
        let entries_before = state.queue.entries.clone();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::MoveEntry { from: 0, to: 2 }),
        );
        assert_eq!(
            state.queue.entries, entries_before,
            "entries must be untouched"
        );
        assert_eq!(state.queue.play_order, vec![1, 2, 0]);
    }

    fn playing_three_track_queue() -> AppState {
        let mut state = tracks_state();
        for i in 0..3 {
            state.active_column_mut().unwrap().cursor = i;
            dispatch(
                &mut state,
                Action::Queue(QueueAction::QueueSelection {
                    full_context: false,
                }),
            );
        }
        state
    }

    #[test]
    fn repeat_one_replays_same_entry() {
        let mut state = playing_three_track_queue();
        state.queue.repeat = RepeatMode::One;
        let current_before = state.player.current;
        let effects = dispatch(
            &mut state,
            Action::Audio(crate::action::AudioEvent::TrackEnded { natural: true }),
        );
        assert_eq!(state.queue.position, 0);
        assert_eq!(state.player.current, current_before);
        // `06-05`: a natural `TrackEnded` also always records a history entry (a `Cache` effect).
        // `06-07`: and always reports "Stopped" for the entry that just finished. `06-06`: under
        // `Repeat::One` the preload target is the entry itself (about to replay), which differs
        // from what was last preloaded during setup (the *next* entry, queued under `Repeat::Off`
        // before this test switched modes) — so a fresh `Preload` follows the reload's `Load`.
        // `08-03`: `load_current` (inside `advance_on_track_ended`) also now emits its own
        // `EnsureCached` `Cache` effect immediately before `Load` — a second, distinct one from
        // the history-append above, not the same effect appearing twice.
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(history),
                Effect::Net(NetEffect::ReportPlayback(_)),
                Effect::Cache(ensure_cached),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(SysEffect::Notify(_)),
                Effect::Sys(SysEffect::UpdateMpris(_)),
                Effect::Audio(AudioEffect::Preload { .. }),
                // `cache.prefetch_next` defaults to one track ahead, so the read-ahead run for
                // whatever is beyond the preloaded entry follows every `Preload`.
                Effect::Cache(prefetch)
            ] if matches!(**history, CacheEffect::AppendHistory(_))
                && matches!(**prefetch, CacheEffect::PrefetchAhead { .. })
                && matches!(**ensure_cached, CacheEffect::EnsureCached { .. })
        ));
    }

    #[test]
    fn repeat_all_wraps_at_end() {
        let mut state = playing_three_track_queue();
        state.queue.repeat = RepeatMode::All;
        state.queue.position = 2; // last entry
        let effects = dispatch(
            &mut state,
            Action::Audio(crate::action::AudioEvent::TrackEnded { natural: true }),
        );
        assert_eq!(state.queue.position, 0);
        // `08-03`: `load_current` also emits its own `EnsureCached` before `Load` — a second,
        // distinct `Cache` effect from the history-append's.
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(history),
                Effect::Net(NetEffect::ReportPlayback(_)),
                Effect::Cache(ensure_cached),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(SysEffect::Notify(_)),
                Effect::Sys(SysEffect::UpdateMpris(_))
            ] if matches!(**history, CacheEffect::AppendHistory(_))
                && matches!(**ensure_cached, CacheEffect::EnsureCached { .. })
        ));
    }

    #[test]
    fn auto_advance_stops_at_end_when_repeat_off() {
        let mut state = playing_three_track_queue();
        state.queue.repeat = RepeatMode::Off;
        state.queue.position = 2;
        let effects = dispatch(
            &mut state,
            Action::Audio(crate::action::AudioEvent::TrackEnded { natural: true }),
        );
        assert_eq!(state.queue.position, 2, "position stays on the last entry");
        assert_eq!(state.player.status, PlayStatus::Stopped);
        // No `Load` (nothing left to play), but `06-05`'s history recording and `06-07`'s
        // "Stopped" report still fire — a natural end always produces both regardless of what
        // happens to the queue afterwards.
        assert!(matches!(
            effects.as_slice(),
            [Effect::Cache(_), Effect::Net(NetEffect::ReportPlayback(_))]
        ));
    }

    #[test]
    fn unnatural_track_ended_does_not_advance() {
        let mut state = playing_three_track_queue();
        let position_before = state.queue.position;
        let effects = dispatch(
            &mut state,
            Action::Audio(crate::action::AudioEvent::TrackEnded { natural: false }),
        );
        assert_eq!(state.queue.position, position_before);
        // `06-07`: "Stopped" is reported on `TrackEnded` in either form, even though an unnatural
        // end never advances or records history.
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::ReportPlayback(_))]
        ));
    }

    #[test]
    fn next_skips_past_repeat_one() {
        let mut state = playing_three_track_queue();
        state.queue.repeat = RepeatMode::One;
        let effects = dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Next),
        );
        assert_eq!(
            state.queue.position, 1,
            "an explicit Next must move forward even under Repeat::One"
        );
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(_),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(SysEffect::Notify(_)),
                Effect::Sys(SysEffect::UpdateMpris(_))
            ]
        ));
    }

    #[test]
    fn prev_restarts_after_three_seconds() {
        let mut state = playing_three_track_queue();
        state.queue.position = 1;
        state.player.position = std::time::Duration::from_secs(5);
        dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Prev),
        );
        assert_eq!(
            state.queue.position, 1,
            "restarts the current entry, does not move back"
        );
    }

    #[test]
    fn prev_moves_back_before_three_seconds() {
        let mut state = playing_three_track_queue();
        state.queue.position = 1;
        state.player.position = std::time::Duration::from_secs(1);
        dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Prev),
        );
        assert_eq!(state.queue.position, 0);
    }

    #[test]
    fn offline_refuses_to_queue_unavailable_track() {
        let mut state = tracks_state();
        state.connectivity = Connectivity::Offline;
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(state.queue.entries.is_empty());
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("not available offline"))
        );
    }

    #[test]
    fn mixed_selection_queues_available_and_toasts_count() {
        let mut state = fixtures::fixture_empty();
        state.connectivity = Connectivity::Offline;
        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let candidates = vec![
            (
                fixtures::track("One", 1, &alb, &[&a]),
                QueueSource::Manual,
                Availability::Cached,
            ),
            (
                fixtures::track("Two", 2, &alb, &[&a]),
                QueueSource::Manual,
                Availability::Unavailable,
            ),
            (
                fixtures::track("Three", 3, &alb, &[&a]),
                QueueSource::Manual,
                Availability::Downloaded,
            ),
        ];
        let effects = append_tracks_gated(&mut state, candidates);
        assert_eq!(state.queue.entries.len(), 2);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(_),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(SysEffect::Notify(_)),
                Effect::Sys(SysEffect::UpdateMpris(_))
            ]
        ));
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains('1') && t.message.contains("skipped"))
        );
    }

    // --- 06-02: appears-on queue rules ------------------------------------------------------

    /// `fixture_appears_on()`'s Albums column, cursor moved to `index` and focused.
    fn appears_on_state(index: usize) -> AppState {
        let mut state = focus_column(fixtures::fixture_appears_on(), Tab::Artists, 0);
        state.active_column_mut().unwrap().cursor = index;
        state
    }

    /// Rebuilds `comp1` — same builder call `fixture_appears_on()` uses internally, so the id
    /// matches (`album()`/`appears_on_album()` derive it from `name` alone).
    fn comp1(main_artist: &crate::model::Artist) -> crate::model::Album {
        fixtures::appears_on_album("Fahrenheit Project, Part Five", 2005, main_artist)
    }

    fn primary1(main_artist: &crate::model::Artist) -> crate::model::Album {
        fixtures::album("Comfortable Void", 2012, main_artist)
    }

    /// 10 tracks for a compilation: 2 credited to `main_artist`, 8 to unrelated artists, in an
    /// order that is deliberately *not* disc/track order, so ordering tests can tell the
    /// difference between "reply order" and "album order".
    fn compilation_tracks(
        comp: &crate::model::Album,
        main_artist: &crate::model::Artist,
    ) -> Vec<Track> {
        let other = fixtures::artist("Various Contributor");
        let mut tracks = Vec::new();
        for n in 1..=10u32 {
            let by_main = n == 3 || n == 7;
            let artists: [&crate::model::Artist; 1] =
                if by_main { [main_artist] } else { [&other] };
            tracks.push(fixtures::track(&format!("Track {n}"), n, comp, &artists));
        }
        // Shuffle away from ascending track-number order.
        tracks.reverse();
        tracks
    }

    fn tracks_loaded(
        state: &mut AppState,
        tracks: Vec<Track>,
        album: &ItemId,
        full_context: bool,
    ) -> Vec<Effect> {
        dispatch(
            state,
            Action::Data(DataAction::TracksLoaded {
                tracks,
                source: QueueSource::Album { id: album.clone() },
                full_context,
            }),
        )
    }

    #[test]
    fn appears_on_enter_queues_only_artist_tracks() {
        let main_artist = fixtures::artist("Sync24");
        let comp = comp1(&main_artist);
        let mut state = appears_on_state(3); // comp1's row

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::FetchAlbumTracksForQueue { album }) ] if *album == comp.id
        ));

        let tracks = compilation_tracks(&comp, &main_artist);
        tracks_loaded(&mut state, tracks, &comp.id, false);

        assert_eq!(state.queue.entries.len(), 2);
        assert!(
            state
                .queue
                .entries
                .iter()
                .all(|e| e.track.artist_ids.contains(&main_artist.id))
        );
    }

    #[test]
    fn appears_on_shift_enter_queues_full_compilation() {
        let main_artist = fixtures::artist("Sync24");
        let comp = comp1(&main_artist);
        let mut state = appears_on_state(3);

        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection { full_context: true }),
        );
        let tracks = compilation_tracks(&comp, &main_artist);
        tracks_loaded(&mut state, tracks, &comp.id, true);

        assert_eq!(state.queue.entries.len(), 10);
    }

    #[test]
    fn queue_full_context_preserves_compilation_order() {
        let main_artist = fixtures::artist("Sync24");
        let comp = comp1(&main_artist);
        let mut state = appears_on_state(3);

        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection { full_context: true }),
        );
        let tracks = compilation_tracks(&comp, &main_artist); // reply order: track 10 down to 1
        tracks_loaded(&mut state, tracks, &comp.id, true);

        let numbers: Vec<u32> = state
            .queue
            .play_order
            .iter()
            .map(|&i| state.queue.entries[i].track.track_number.unwrap())
            .collect();
        assert_eq!(numbers, (1..=10).collect::<Vec<_>>());
    }

    #[test]
    fn filtered_tracks_keep_disc_and_track_order() {
        let main_artist = fixtures::artist("Sync24");
        let comp = comp1(&main_artist);
        let mut state = appears_on_state(3);

        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let tracks = compilation_tracks(&comp, &main_artist); // main_artist on tracks 3 and 7
        tracks_loaded(&mut state, tracks, &comp.id, false);

        let numbers: Vec<u32> = state
            .queue
            .play_order
            .iter()
            .map(|&i| state.queue.entries[i].track.track_number.unwrap())
            .collect();
        assert_eq!(numbers, vec![3, 7]);
    }

    #[test]
    fn primary_album_ignores_full_context_flag() {
        let main_artist = fixtures::artist("Sync24");
        let primary = primary1(&main_artist);
        let make_tracks = || {
            vec![
                fixtures::track("A", 1, &primary, &[&main_artist]),
                fixtures::track("B", 2, &primary, &[&main_artist]),
                fixtures::track("C", 3, &primary, &[&main_artist]),
            ]
        };

        let mut enter_state = appears_on_state(0); // primary1's row
        dispatch(
            &mut enter_state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        tracks_loaded(&mut enter_state, make_tracks(), &primary.id, false);

        let mut shift_enter_state = appears_on_state(0);
        dispatch(
            &mut shift_enter_state,
            Action::Queue(QueueAction::QueueSelection { full_context: true }),
        );
        tracks_loaded(&mut shift_enter_state, make_tracks(), &primary.id, true);

        assert_eq!(enter_state.queue.entries.len(), 3);
        assert_eq!(
            enter_state.queue.entries.len(),
            shift_enter_state.queue.entries.len()
        );
    }

    #[test]
    fn artist_selection_queues_all_artist_tracks() {
        let a1 = fixtures::artist("Boy Harsher");
        let mut artists_col = Column::new(ColumnKind::Artists, "Artists");
        artists_col.items = vec![MediaItem::Artist(a1.clone())];
        let mut state = fixtures::fixture_empty();
        state
            .nav
            .per_tab_stacks
            .insert(Tab::Artists, vec![artists_col]);
        let mut state = focus_column(state, Tab::Artists, 0);
        state.active_column_mut().unwrap().cursor = 0;

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::FetchArtistTracksForQueue { artist })] if *artist == a1.id
        ));

        let alb = fixtures::album("Anthology", 2018, &a1);
        let tracks = vec![
            fixtures::track("One", 1, &alb, &[&a1]),
            fixtures::track("Two", 2, &alb, &[&a1]),
        ];
        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks,
                source: QueueSource::Artist { id: a1.id.clone() },
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 2);
    }

    // --- 07-04: genres tab ---------------------------------------------------------------

    /// Sorting used to reorder `entries` in place, destroying the order the tracks were queued in
    /// — so there was no way back to it (`docs/12-decisions.md`). It permutes `play_order` now, the
    /// way shuffle always has, which makes the default order recoverable.
    #[test]
    fn a_sort_can_be_undone_back_to_the_queued_order() {
        let mut state = fixtures::fixture_empty();
        let artist = fixtures::artist("Boy Harsher");
        let album = fixtures::album("Care", 2019, &artist);
        // Queued in album order, which is what "default" means for an album.
        let tracks = vec![
            fixtures::track("first", 1, &album, &[&artist]),
            fixtures::track("second", 2, &album, &[&artist]),
            fixtures::track("third", 3, &album, &[&artist]),
        ];
        deliver(
            &mut state,
            tracks,
            QueueSource::Album {
                id: album.id.clone(),
            },
        );
        let default_order: Vec<String> =
            queued_names(&state).iter().map(|s| s.to_string()).collect();
        assert_eq!(default_order, vec!["first", "second", "third"]);

        // A title-descending profile visibly reorders it.
        state
            .config
            .sorting
            .profiles
            .push(crate::config::SortProfile {
                name: "by name desc".to_string(),
                rules: vec![crate::config::SortRule {
                    field: crate::config::SortField::Name,
                    direction: crate::config::Direction::Desc,
                }],
            });
        dispatch(
            &mut state,
            Action::Queue(QueueAction::ApplySortProfile("by name desc".to_string())),
        );
        assert_ne!(
            queued_names(&state),
            default_order,
            "the sort must do something"
        );
        assert_eq!(state.queue.sort_profile.as_deref(), Some("by name desc"));

        dispatch(&mut state, Action::Queue(QueueAction::RestoreDefaultOrder));

        assert_eq!(
            queued_names(&state),
            default_order,
            "the queued order must come back exactly"
        );
        assert_eq!(state.queue.sort_profile, None, "and no profile is active");
    }

    /// The track playing keeps playing across both directions — the same guarantee sorting and
    /// shuffling already made.
    #[test]
    fn restoring_the_default_order_keeps_the_current_track() {
        let mut state = fixtures::fixture_playing_queue();
        let playing = state
            .current_entry()
            .expect("something is playing")
            .entry_id;
        state
            .config
            .sorting
            .profiles
            .push(crate::config::SortProfile {
                name: "by name desc".to_string(),
                rules: vec![crate::config::SortRule {
                    field: crate::config::SortField::Name,
                    direction: crate::config::Direction::Desc,
                }],
            });

        dispatch(
            &mut state,
            Action::Queue(QueueAction::ApplySortProfile("by name desc".to_string())),
        );
        assert_eq!(state.current_entry().map(|e| e.entry_id), Some(playing));

        dispatch(&mut state, Action::Queue(QueueAction::RestoreDefaultOrder));
        assert_eq!(state.current_entry().map(|e| e.entry_id), Some(playing));
    }

    // --- favourites ------------------------------------------------------------------------------

    /// A favourited playlist is playable straight from the Favourites tab, like every other row
    /// there — `container_fetch` already understood a `Playlist`, it just never saw one from here.
    #[test]
    fn enter_on_a_favourited_playlist_fetches_its_tracks() {
        let playlist = crate::model::Playlist {
            id: ItemId::from("pl1"),
            name: "Mix".to_string(),
            overview: None,
            track_count: 3,
            total_duration: std::time::Duration::from_secs(600),
            can_edit: true,
            is_favorite: true,
        };
        let mut state = favourites_state(vec![], vec![], vec![]);
        state.favourites.results.playlists = vec![playlist.clone()];
        state.favourites.focused_section = SearchSection::Playlists;

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::FetchPlaylistTracksForQueue { playlist: id })
                    if id.as_str() == playlist.id.as_str()
            )),
            "got {effects:?}"
        );
    }

    /// Unfavouriting one from the tab removes its row optimistically, and a failed request puts it
    /// back where it was — the same contract the other three sections already had.
    #[test]
    fn unfavouriting_a_playlist_removes_and_restores_its_row() {
        let playlist = crate::model::Playlist {
            id: ItemId::from("pl1"),
            name: "Mix".to_string(),
            overview: None,
            track_count: 3,
            total_duration: std::time::Duration::from_secs(600),
            can_edit: true,
            is_favorite: true,
        };
        let mut state = favourites_state(vec![], vec![], vec![]);
        state.favourites.results.playlists = vec![playlist.clone()];
        state.favourites.focused_section = SearchSection::Playlists;

        dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert!(state.favourites.results.playlists.is_empty());

        dispatch(
            &mut state,
            Action::Data(DataAction::LoadFailed {
                target: crate::action::LoadTarget::FavoriteToggle(playlist.id.clone()),
                message: "boom".to_string(),
                offline: false,
            }),
        );
        assert_eq!(state.favourites.results.playlists.len(), 1);
    }

    /// Emby favourites any item type, playlists included — but `resolve_favorite_target` matched
    /// only artists, albums and tracks, so `f` on a playlist row did nothing (`docs/12-decisions.md`).
    #[test]
    fn f_favourites_a_playlist_row() {
        let playlist = crate::model::Playlist {
            id: ItemId::from("pl1"),
            name: "Спи наш милый Костик".to_string(),
            overview: None,
            track_count: 14,
            total_duration: std::time::Duration::from_secs(3000),
            can_edit: true,
            is_favorite: false,
        };
        let mut column = Column::new(ColumnKind::Playlists, "Playlists");
        column.items = vec![MediaItem::Playlist(playlist.clone())];
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Playlists;
        state
            .nav
            .per_tab_stacks
            .insert(Tab::Playlists, vec![column]);
        let mut state = focus_column(state, Tab::Playlists, 0);

        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::SetFavorite { id, on: true }) if *id == playlist.id
            )),
            "got {effects:?}"
        );
        // And the optimistic flag lands on the row, so the heart shows straight away.
        let column = state.active_column().unwrap();
        assert!(matches!(&column.items[0], MediaItem::Playlist(p) if p.is_favorite));
    }

    /// Neither play view has a Miller column, so `selected_item` returns `None` on both and `f`
    /// did nothing at all — you could not favourite the track you were listening to from the two
    /// screens that exist to show it (`docs/12-decisions.md`).
    #[test]
    fn f_favourites_the_track_a_play_view_is_showing() {
        for zen in [false, true] {
            let mut state = fixtures::fixture_playing_queue();
            state.nav.active_tab = Tab::NowPlaying;
            state.zen_mode = zen;
            let playing = state.queue.current().unwrap().track.id.clone();
            state.now_playing_cursor = state.queue.position;

            let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
            assert!(
                effects.iter().any(|e| matches!(
                    e,
                    Effect::Net(NetEffect::SetFavorite { id, on: true }) if *id == playing
                )),
                "zen={zen}: got {effects:?}"
            );
        }
    }

    /// In the queue pane it is the row under the cursor, not merely whatever happens to be playing.
    #[test]
    fn now_playing_favourites_the_row_under_the_cursor() {
        let mut state = fixtures::fixture_playing_queue();
        state.nav.active_tab = Tab::NowPlaying;
        state.now_playing_cursor = state.queue.position + 2;
        let expected = {
            let index = state.queue.play_order[state.now_playing_cursor];
            state.queue.entries[index].track.id.clone()
        };

        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::SetFavorite { id, .. }) if *id == expected
            )),
            "got {effects:?}"
        );
    }

    /// The heart appeared on the row but the track never turned up in the Favourites tab: that list
    /// is fetched once on first entry and nothing ever invalidated it (`docs/12-decisions.md`).
    #[test]
    fn favouriting_elsewhere_makes_the_favourites_tab_reload() {
        let mut state = fixtures::fixture_playing_queue();
        state.nav.active_tab = Tab::NowPlaying;
        state.favourites.load = crate::state::nav::LoadState::Loaded { total: 3 };

        dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert_eq!(state.favourites.load, crate::state::nav::LoadState::Idle);

        // …and the next visit to the tab actually refetches.
        let effects = dispatch(
            &mut state,
            Action::Nav(crate::action::NavAction::SetTab(Tab::Favourites)),
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Net(NetEffect::FetchFavourites))),
            "got {effects:?}"
        );
    }

    /// Unfavouriting *from* the Favourites tab must not invalidate it: the row has already been
    /// removed optimistically, and a refetch would only make the list flicker.
    #[test]
    fn unfavouriting_from_the_favourites_tab_does_not_reload_it() {
        let artist = fixtures::artist("Sync24");
        let mut state = favourites_state(vec![artist.clone()], vec![], vec![]);
        state.favourites.results.artists[0].is_favorite = true;
        state.favourites.focused_section = SearchSection::Artists;
        state.favourites.load = crate::state::nav::LoadState::Loaded { total: 1 };

        dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert_eq!(
            state.favourites.load,
            crate::state::nav::LoadState::Loaded { total: 1 }
        );
        assert!(state.favourites.results.artists.is_empty(), "row removed");
    }

    // --- downloads -----------------------------------------------------------------------------

    /// `d` was bound, offered by the inspector and listed in the help sheet, and its reducer arm
    /// returned no effects at all — so it did nothing, anywhere (`docs/12-decisions.md`).
    #[test]
    fn d_pins_an_album_and_unpins_it_again() {
        let (mut state, first, _second) = two_album_state();

        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleDownload));
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Cache(c) if matches!(&**c, CacheEffect::PinDownload {
                    scope: crate::effect::DownloadScope::Album(id)
                } if *id == first.id)
            )),
            "an album row must be downloadable, got {effects:?}"
        );
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("downloading"))
        );

        // Once it is pinned, the same key gives it back.
        state.downloads.insert(first.id.clone());
        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleDownload));
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Cache(c) if matches!(&**c, CacheEffect::RemoveDownload { id } if *id == first.id)
            )),
            "got {effects:?}"
        );
    }

    /// "It doesn't work in select mode either" — every selected row gets its own request.
    #[test]
    fn d_downloads_every_row_of_a_multi_selection() {
        let (mut state, first, second) = two_album_state();
        {
            let column = state.active_column_mut().unwrap();
            column.selection.visual_mode = true;
            column.selection.selected.insert(first.id.clone());
            column.selection.selected.insert(second.id.clone());
        }

        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleDownload));
        let pinned: Vec<&ItemId> = effects
            .iter()
            .filter_map(|e| match e {
                Effect::Cache(c) => match &**c {
                    CacheEffect::PinDownload {
                        scope: crate::effect::DownloadScope::Album(id),
                    } => Some(id),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(pinned, vec![&first.id, &second.id]);
    }

    #[test]
    fn d_refuses_while_offline() {
        let (mut state, _first, _second) = two_album_state();
        state.connectivity = Connectivity::Offline;

        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleDownload));
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("connection"))
        );
    }

    // --- the Favourites tab -------------------------------------------------------------------

    /// Favourites reuses Search's three-section shape, but only Search was ever wired into the
    /// queueing paths: `AppState::selected_item` returns `None` on a tab with no Miller column, so
    /// `Enter`, `a`, `i` and `m` all did nothing whatsoever on Favourites (`docs/12-decisions.md`).
    fn favourites_state(
        artists: Vec<crate::model::Artist>,
        albums: Vec<crate::model::Album>,
        tracks: Vec<Track>,
    ) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Favourites;
        state.favourites.results = crate::state::search::SearchResults {
            artists,
            albums,
            tracks,
            artists_error: None,
            albums_error: None,
            tracks_error: None,
            playlists: Vec::new(),
            playlists_error: None,
        };
        state
    }

    #[test]
    fn enter_on_a_favourite_track_queues_it() {
        let artist = fixtures::artist("Sync24");
        let album = fixtures::album("Source", 2007, &artist);
        let track = fixtures::track("Bloom", 1, &album, &[&artist]);
        let mut state = favourites_state(vec![], vec![], vec![track.clone()]);
        state.favourites.focused_section = SearchSection::Tracks;

        dispatch(
            &mut state,
            Action::Queue(QueueAction::PlaySelection {
                full_context: false,
            }),
        );
        assert_eq!(queued_names(&state), vec!["Bloom"]);
    }

    #[test]
    fn enter_on_a_favourite_album_fetches_its_tracks() {
        let artist = fixtures::artist("Sync24");
        let album = fixtures::album("Source", 2007, &artist);
        let mut state = favourites_state(vec![], vec![album.clone()], vec![]);
        state.favourites.focused_section = SearchSection::Albums;

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::FetchAlbumTracksForQueue { album: id }) if id == &album.id
            )),
            "got {effects:?}"
        );
    }

    /// The same insert-vs-append round trip as a column row, from a tab that had no queueing at all.
    #[test]
    fn insert_next_on_a_favourite_album_inserts_after_the_current_track() {
        let artist = fixtures::artist("Sync24");
        let album = fixtures::album("Source", 2007, &artist);
        let sitting = fixtures::track("sitting", 1, &album, &[&artist]);
        let later = fixtures::track("later", 2, &album, &[&artist]);
        let mut state = favourites_state(vec![], vec![album.clone()], vec![]);
        state.favourites.focused_section = SearchSection::Albums;
        append_tracks(
            &mut state,
            vec![(sitting, QueueSource::Manual), (later, QueueSource::Manual)],
        );

        dispatch(&mut state, Action::Queue(QueueAction::InsertNext));
        let inserted = fixtures::album("Comfortable Void", 2012, &artist);
        deliver(
            &mut state,
            vec![fixtures::track("new one", 1, &inserted, &[&artist])],
            QueueSource::Album {
                id: album.id.clone(),
            },
        );
        assert_eq!(queued_names(&state), vec!["sitting", "new one", "later"]);
    }

    #[test]
    fn instant_mix_seeds_from_the_focused_favourite() {
        let artist = fixtures::artist("Sync24");
        let mut state = favourites_state(vec![artist.clone()], vec![], vec![]);
        state.favourites.focused_section = SearchSection::Artists;

        let effects = dispatch(&mut state, Action::Queue(QueueAction::InstantMix));
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::InstantMix { seed, .. }) if seed == &artist.id
            )),
            "got {effects:?}"
        );
    }

    // --- container queueing: `i` on a container, and multi-selected containers ---------------

    /// Two albums in one Albums column, plus the tracks each will answer with.
    fn two_album_state() -> (AppState, crate::model::Album, crate::model::Album) {
        let artist = fixtures::artist("Boy Harsher");
        let first = fixtures::album("Aaa", 2014, &artist);
        let second = fixtures::album("Bbb", 2019, &artist);
        let mut column = Column::new(
            ColumnKind::Albums {
                of_artist: Some(artist.id.clone()),
            },
            "Albums",
        );
        column.items = vec![
            MediaItem::Album(first.clone()),
            MediaItem::Album(second.clone()),
        ];
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Albums;
        state.nav.per_tab_stacks.insert(Tab::Albums, vec![column]);
        let state = focus_column(state, Tab::Albums, 0);
        (state, first, second)
    }

    fn album_tracks(album: &crate::model::Album, names: &[&str]) -> Vec<Track> {
        let artist = fixtures::artist("Boy Harsher");
        names
            .iter()
            .enumerate()
            .map(|(i, name)| fixtures::track(name, i as u32 + 1, album, &[&artist]))
            .collect()
    }

    /// `i` on an album row did nothing whatsoever: it went straight to `resolve_selection_tracks`,
    /// which kept only `Track` rows, found none, and returned — "`i` inserts tracks, not albums or
    /// artists" (`docs/12-decisions.md`). It must fetch, and the reply must be *inserted* after the
    /// current track rather than appended, which is the part no reply can work out on its own.
    #[test]
    fn insert_next_on_an_album_row_inserts_the_fetched_tracks() {
        let (mut state, first, second) = two_album_state();

        // A queue already playing "sitting" with "later" behind it.
        state.active_column_mut().unwrap().cursor = 1;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        deliver(
            &mut state,
            album_tracks(&second, &["sitting", "later"]),
            QueueSource::Album {
                id: second.id.clone(),
            },
        );
        assert_eq!(queued_names(&state), vec!["sitting", "later"]);

        state.active_column_mut().unwrap().cursor = 0;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::InsertNext));
        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::FetchAlbumTracksForQueue { album }) if album == &first.id
            )),
            "`i` on an album must fetch its tracks, got {effects:?}"
        );

        deliver(
            &mut state,
            album_tracks(&first, &["one", "two"]),
            QueueSource::Album {
                id: first.id.clone(),
            },
        );
        assert_eq!(
            queued_names(&state),
            vec!["sitting", "one", "two", "later"],
            "the album must land after the current track, not at the end"
        );
    }

    /// `v`-selecting albums and pressing `a` queued nothing at all — `resolve_selection_tracks`
    /// dropped every non-`Track` row, so the candidate list came back empty and the press was
    /// silently discarded (`docs/12-decisions.md`).
    ///
    /// The replies are delivered **second album first**, deliberately: the queue must come out in
    /// the order the rows are displayed, not the order the server happened to answer in.
    #[test]
    fn multi_selected_albums_queue_in_display_order_whatever_order_replies_arrive() {
        let (mut state, first, second) = two_album_state();
        {
            let column = state.active_column_mut().unwrap();
            column.selection.visual_mode = true;
            column.selection.selected.insert(first.id.clone());
            column.selection.selected.insert(second.id.clone());
        }

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let fetched: Vec<&ItemId> = effects
            .iter()
            .filter_map(|e| match e {
                Effect::Net(NetEffect::FetchAlbumTracksForQueue { album }) => Some(album),
                _ => None,
            })
            .collect();
        assert_eq!(
            fetched,
            vec![&first.id, &second.id],
            "both selected albums must be fetched"
        );

        deliver(
            &mut state,
            album_tracks(&second, &["b one", "b two"]),
            QueueSource::Album {
                id: second.id.clone(),
            },
        );
        assert!(
            state.queue.entries.is_empty(),
            "nothing may be queued until every reply is in — otherwise the order is the server's"
        );

        deliver(
            &mut state,
            album_tracks(&first, &["a one", "a two"]),
            QueueSource::Album {
                id: first.id.clone(),
            },
        );
        assert_eq!(
            queued_names(&state),
            vec!["a one", "a two", "b one", "b two"]
        );
    }

    /// Slot ordering with a mix of fetched and already-in-hand rows: the track must hold its place
    /// in the running order rather than jumping ahead of the album above it.
    ///
    /// `.`/`V` no longer *build* a mixed selection — one kind at a time, by user request
    /// (`reducer::nav::toggle_item`) — so this drives the batch directly. The guarantee is kept
    /// because nothing at queue time re-checks the rule, and a batch must never reorder its slots.
    #[test]
    fn a_mixed_selection_keeps_tracks_in_their_displayed_position() {
        let artist = fixtures::artist("Boy Harsher");
        let album = fixtures::album("Aaa", 2014, &artist);
        let loose = fixtures::track("loose", 9, &album, &[&artist]);
        let mut column = Column::new(
            ColumnKind::Albums {
                of_artist: Some(artist.id.clone()),
            },
            "Mixed",
        );
        column.items = vec![
            MediaItem::Album(album.clone()),
            MediaItem::Track(loose.clone()),
        ];
        column.selection.visual_mode = true;
        column.selection.selected.insert(album.id.clone());
        column.selection.selected.insert(loose.id.clone());
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Albums;
        state.nav.per_tab_stacks.insert(Tab::Albums, vec![column]);
        let mut state = focus_column(state, Tab::Albums, 0);

        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        deliver(
            &mut state,
            album_tracks(&album, &["one", "two"]),
            QueueSource::Album {
                id: album.id.clone(),
            },
        );
        assert_eq!(queued_names(&state), vec!["one", "two", "loose"]);
    }

    /// A failed fetch carries no source, so the batch it belongs to can never complete. It must be
    /// flushed with whatever did arrive rather than parked forever — otherwise one failure among
    /// several albums silently discards all of them.
    #[test]
    fn a_failed_fetch_queues_the_rest_of_the_batch_instead_of_wedging_it() {
        let (mut state, first, second) = two_album_state();
        {
            let column = state.active_column_mut().unwrap();
            column.selection.visual_mode = true;
            column.selection.selected.insert(first.id.clone());
            column.selection.selected.insert(second.id.clone());
        }
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        deliver(
            &mut state,
            album_tracks(&first, &["a one"]),
            QueueSource::Album {
                id: first.id.clone(),
            },
        );
        assert!(
            state.queue.entries.is_empty(),
            "the batch is still waiting on the second album"
        );

        dispatch(
            &mut state,
            Action::Data(DataAction::LoadFailed {
                target: crate::action::LoadTarget::QueueFetch,
                message: "boom".to_string(),
                offline: false,
            }),
        );
        assert_eq!(queued_names(&state), vec!["a one"]);
        assert!(state.pending_queue_batch.is_none());
        assert!(
            state.pending_album_queue_fetch.is_empty(),
            "the abandoned album's pending filter must not leak"
        );
    }

    /// Two albums by one artist, deliberately interleaved on arrival, so a per-track `(disc, track)`
    /// sort would visibly scramble them into "every track 1, then every track 2".
    fn two_albums_interleaved() -> Vec<Track> {
        let artist = fixtures::artist("Boy Harsher");
        let early = fixtures::album("Early", 2014, &artist);
        let late = fixtures::album("Late", 2019, &artist);
        vec![
            fixtures::track("late one", 1, &late, &[&artist]),
            fixtures::track("early one", 1, &early, &[&artist]),
            fixtures::track("late two", 2, &late, &[&artist]),
            fixtures::track("early two", 2, &early, &[&artist]),
        ]
    }

    fn queued_names(state: &AppState) -> Vec<&str> {
        state
            .queue
            .play_order
            .iter()
            .map(|&i| state.queue.entries[i].track.name.as_str())
            .collect()
    }

    fn deliver(state: &mut AppState, tracks: Vec<Track>, source: QueueSource) {
        dispatch(
            state,
            Action::Data(DataAction::TracksLoaded {
                tracks,
                source,
                full_context: true,
            }),
        );
    }

    /// A genre spans many albums, so it follows the user's own queue sort profile — the default
    /// `chronological_discog` being album-artist, year, album, track (`docs/12-decisions.md`).
    #[test]
    fn a_queued_genre_follows_the_configured_sort_profile() {
        let mut state = fixtures::fixture_empty();
        deliver(
            &mut state,
            two_albums_interleaved(),
            QueueSource::Genre {
                name: "Darkwave".to_string(),
            },
        );

        assert_eq!(
            queued_names(&state),
            vec!["early one", "early two", "late one", "late two"],
            "albums must stay whole and in year order, not be interleaved by track number"
        );
    }

    /// An artist spans albums too, and had the identical defect: server-side album ordering was
    /// undone by a per-track `(disc, track)` re-sort.
    #[test]
    fn a_queued_artist_follows_the_configured_sort_profile() {
        let mut state = fixtures::fixture_empty();
        deliver(
            &mut state,
            two_albums_interleaved(),
            QueueSource::Artist {
                id: ItemId::from("artist-Boy Harsher"),
            },
        );

        assert_eq!(
            queued_names(&state),
            vec!["early one", "early two", "late one", "late two"]
        );
    }

    /// Changing the configured profile changes the queued order — that is the whole request.
    #[test]
    fn a_different_profile_gives_a_different_genre_order() {
        let mut state = fixtures::fixture_empty();
        // `release_chronology` is year **descending**, so the later album comes first.
        state.config.sorting.default_queue_profile = "release_chronology".to_string();
        deliver(
            &mut state,
            two_albums_interleaved(),
            QueueSource::Genre {
                name: "Darkwave".to_string(),
            },
        );

        assert_eq!(
            queued_names(&state),
            vec!["late one", "late two", "early one", "early two"]
        );
    }

    /// An album keeps album order regardless of the profile: pressing `Enter` on an album means
    /// "play this album", not "apply my library sort to it".
    #[test]
    fn a_queued_album_keeps_disc_and_track_order() {
        let mut state = fixtures::fixture_empty();
        state.config.sorting.default_queue_profile = "release_chronology".to_string();
        let artist = fixtures::artist("Boy Harsher");
        let album = fixtures::album("Care", 2019, &artist);
        let tracks = vec![
            fixtures::track("second", 2, &album, &[&artist]),
            fixtures::track("first", 1, &album, &[&artist]),
        ];

        deliver(
            &mut state,
            tracks,
            QueueSource::Album {
                id: album.id.clone(),
            },
        );

        assert_eq!(queued_names(&state), vec!["first", "second"]);
    }

    /// A genre used to be refused outright, on the grounds that one can span tens of thousands of
    /// tracks. A user selecting a genre is asking for exactly that, and said so — it queues the
    /// whole thing now, and the append toast reports the count (`docs/12-decisions.md`).
    #[test]
    fn queueing_a_genre_fetches_all_of_its_tracks() {
        let genre = MediaItem::Genre(crate::model::Genre {
            id: ItemId::from("genre-darkwave"),
            name: "Darkwave".to_string(),
        });
        let mut state = single_item_column(ColumnKind::Genres, genre);

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );

        assert!(
            effects.iter().any(|e| matches!(
                e,
                Effect::Net(NetEffect::FetchGenreTracksForQueue { genre }) if genre == "Darkwave"
            )),
            "the genre's tracks must be fetched by name: {effects:?}"
        );
        assert!(
            !state
                .toasts
                .iter()
                .any(|t| t.message.contains("select an artist")),
            "no refusal toast any more"
        );
    }

    /// `Enter` clears the queue before filling it, so a genre must count as queueable — otherwise
    /// the guard against wiping a playing queue would refuse it.
    #[test]
    fn enter_on_a_genre_replaces_the_queue() {
        let genre = MediaItem::Genre(crate::model::Genre {
            id: ItemId::from("genre-darkwave"),
            name: "Darkwave".to_string(),
        });
        let mut state = single_item_column(ColumnKind::Genres, genre);

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::PlaySelection {
                full_context: false,
            }),
        );

        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Net(NetEffect::FetchGenreTracksForQueue { .. })))
        );
    }

    // --- 07-05: folders tab ---------------------------------------------------------------

    fn folder_tracks(n: usize) -> Vec<Track> {
        let a = fixtures::artist("Loose Files");
        let alb = fixtures::album("Untagged", 2020, &a);
        (0..n)
            .map(|i| fixtures::track(&format!("Track {i}"), i as u32, &alb, &[&a]))
            .collect()
    }

    #[test]
    fn queue_folder_is_recursive_and_toasts_count() {
        let folder = MediaItem::Folder(crate::model::Folder {
            id: ItemId::from("folder-1"),
            name: "Live Sets".to_string(),
        });
        let mut state = single_item_column(ColumnKind::Folders { of_parent: None }, folder);

        // Whichever key was pressed, a folder now queues its whole subtree.
        for full_context in [false, true] {
            let effects = dispatch(
                &mut state,
                Action::Queue(QueueAction::QueueSelection { full_context }),
            );
            assert!(
                matches!(
                    effects.as_slice(),
                    [Effect::Net(NetEffect::FetchFolderTracksForQueue { folder, recursive: true })]
                        if folder.as_str() == "folder-1"
                ),
                "folders always fetch recursively (full_context={full_context})"
            );
        }

        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks: folder_tracks(3),
                source: QueueSource::Folder {
                    id: ItemId::from("folder-1"),
                    recursive: true,
                },
                full_context: true,
            }),
        );
        assert_eq!(state.queue.entries.len(), 3);
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("queued 3 tracks from this folder")),
            "the count is always toasted so a big (or empty) subtree is discoverable"
        );
    }

    /// A real bug found in the field: pressing `Enter`/`a` directly on a Playlist row in the
    /// Playlists tab's own top-level list (never having drilled into its tracks) used to fall all
    /// the way through to `resolve_selection_tracks`, which has no `Track` to find on a bare
    /// `Playlist` item — it silently queued nothing at all. Every other container row type
    /// (`Album`/`Artist`/`Folder`) already had this wiring; `Playlist` did not.
    #[test]
    fn queue_playlist_row_fetches_its_tracks() {
        let playlist = MediaItem::Playlist(crate::model::Playlist {
            id: ItemId::from("pl-1"),
            name: "Road Trip".to_string(),
            overview: None,
            track_count: 3,
            total_duration: std::time::Duration::from_secs(600),
            can_edit: true,
            is_favorite: false,
        });
        let mut state = single_item_column(ColumnKind::Playlists, playlist);

        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::Net(NetEffect::FetchPlaylistTracksForQueue { playlist })]
                    if playlist.as_str() == "pl-1"
            ),
            "{effects:?}"
        );

        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks: folder_tracks(3),
                source: QueueSource::Playlist {
                    id: PlaylistId::from("pl-1"),
                },
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 3);
    }

    #[test]
    fn track_selection_queues_one() {
        let mut state = tracks_state();
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 1);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(_),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(SysEffect::Notify(_)),
                Effect::Sys(SysEffect::UpdateMpris(_))
            ]
        ));
    }

    /// `10-10`.
    #[test]
    fn notifies_on_track_change() {
        let mut state = tracks_state();
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let entry = state.queue.current().unwrap().clone();
        let expected = TrackChange {
            title: entry.track.name.clone(),
            artist: entry.track.artist_names.join(", "),
            album: entry.track.album_name.clone(),
            art_path: None,
        };
        assert!(effects.contains(&Effect::Sys(SysEffect::Notify(expected))));
    }

    /// `10-11`: unlike `Notify`, there is no config gate or focus check — `mpris_meta` always
    /// fires on a genuine track change.
    #[test]
    fn metadata_updated_on_track_change() {
        let mut state = tracks_state();
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let entry = state.queue.current().unwrap().clone();
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Sys(SysEffect::UpdateMpris(meta))
                if meta.title == entry.track.name
                    && meta.artist == entry.track.artist_names.join(", ")
                    && meta.album == entry.track.album_name
                    && meta.art_url.is_none()
        )));
    }

    /// `10-10`: `ui.desktop_notifications = false` suppresses the effect entirely — not merely
    /// "the worker drops it silently".
    #[test]
    fn disabled_by_config() {
        let mut state = tracks_state();
        state.config.ui.desktop_notifications = false;
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::Notify(_))))
        );
    }

    /// `10-10`: `Some(true)` — the player bar already shows this, the terminal has focus.
    #[test]
    fn suppressed_when_terminal_focused() {
        let mut state = tracks_state();
        state.terminal_focused = Some(true);
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::Notify(_))))
        );
    }

    /// `10-10`: `None` (no `TerminalFocusChanged` has ever arrived) notifies normally — only a
    /// confirmed `Some(true)` suppresses (`AppState::terminal_focused`'s own doc comment).
    #[test]
    fn notifies_when_focus_unknown() {
        let mut state = tracks_state();
        assert_eq!(state.terminal_focused, None);
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Sys(SysEffect::Notify(_))))
        );
    }

    /// `10-10`: `notify_track_change` is only ever called from inside `load_current` — none of
    /// these three actions touch it (`PlayPause`/`Seek` never call `load_current` at all; the
    /// third `QueueSelection` on an already-current track re-enters `load_current`, which is a
    /// genuine notify per this task's own design, not a plain resume — so it is deliberately
    /// excluded from this list rather than asserted negative here).
    #[test]
    fn does_not_notify_on_pause_resume_or_seek() {
        let mut state = playing_three_track_queue();
        for action in [
            Action::Player(crate::action::PlayerAction::PlayPause),
            Action::Player(crate::action::PlayerAction::Seek(
                crate::state::player::SeekTarget::Relative(5),
            )),
        ] {
            let effects = dispatch(&mut state, action);
            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::Sys(SysEffect::Notify(_)))),
                "must not notify on pause/resume/seek"
            );
        }
    }

    #[test]
    fn multiselect_queues_selected_only() {
        let mut state = focus_column(fixtures::fixture_visual_select(), Tab::Artists, 0);
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 3, "only the 3 selected tracks");
    }

    #[test]
    fn album_row_a_and_shift_a_both_emit_identical_fetch_effect() {
        let effects_a = dispatch(
            &mut appears_on_state(3),
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        let effects_shift_a = dispatch(
            &mut appears_on_state(3),
            Action::Queue(QueueAction::QueueSelection { full_context: true }),
        );
        assert_eq!(effects_a, effects_shift_a);
    }

    #[test]
    fn artist_only_filter_applied_to_reply_not_before_fetch() {
        let effects = dispatch(
            &mut appears_on_state(3),
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        // The effect itself carries no filter — nothing distinguishes `a` from `A` in the
        // request, only in `AppState::pending_album_queue_fetch` and how the reply is handled.
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::FetchAlbumTracksForQueue { .. })]
        ));
    }

    #[test]
    fn track_row_selection_needs_no_fetch() {
        let effects = dispatch(
            &mut tracks_state(),
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(
            !effects.iter().any(|e| matches!(e, Effect::Net(_))),
            "a track already in a loaded column must queue without any fetch"
        );
    }

    #[test]
    fn empty_filter_result_toasts_and_queues_nothing() {
        let main_artist = fixtures::artist("Sync24");
        let comp = comp1(&main_artist);
        let mut state = appears_on_state(3);

        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        // Nobody on this compilation is `main_artist` — the ArtistOnly filter matches nothing.
        let other = fixtures::artist("Nobody Related");
        let tracks = vec![
            fixtures::track("One", 1, &comp, &[&other]),
            fixtures::track("Two", 2, &comp, &[&other]),
        ];
        let effects = tracks_loaded(&mut state, tracks, &comp.id, false);

        assert!(state.queue.entries.is_empty());
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("no tracks by"))
        );
    }

    // --- 06-03: non-destructive shuffle -----------------------------------------------------

    #[test]
    fn append_while_shuffled_goes_to_the_end() {
        let mut state = playing_three_track_queue();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::ToggleShuffle { seed: 1 }),
        );
        assert!(state.queue.shuffled);

        state.active_column_mut().unwrap().cursor = 2;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );

        assert_eq!(
            *state.queue.play_order.last().unwrap(),
            state.queue.entries.len() - 1,
            "a freshly appended entry must land at the very end of play_order, not interleaved"
        );
    }

    #[test]
    fn toggle_shuffle_emits_preload() {
        let mut state = playing_three_track_queue();
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::ToggleShuffle { seed: 1 }),
        );
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Audio(AudioEffect::Preload { .. }),
                Effect::Cache(prefetch)
            ] if matches!(**prefetch, CacheEffect::PrefetchAhead { .. })
        ));
    }

    fn prefetch_run(
        effects: &[Effect],
    ) -> Option<(Vec<crate::model::ItemId>, crate::config::QualityProfile)> {
        effects.iter().find_map(|e| match e {
            Effect::Cache(c) => match &**c {
                CacheEffect::PrefetchAhead { tracks, profile } => {
                    Some((tracks.iter().map(|t| t.id.clone()).collect(), *profile))
                }
                _ => None,
            },
            _ => None,
        })
    }

    /// mpv's own `Preload` reaches one track ahead and keeps nothing on disk, so before
    /// `cache.prefetch_next` the rolling cache only ever held what had already been played
    /// (`docs/12-decisions.md`). The read-ahead must cover the next N *upcoming* entries, in play
    /// order, at the profile the current track is playing at.
    #[test]
    fn prefetch_covers_the_next_n_upcoming_tracks_at_the_active_profile() {
        let mut state = fixtures::fixture_playing_queue(); // 10 entries, position 3
        state.config.cache.prefetch_next = 3;
        state.player.quality_profile = crate::config::QualityProfile::TranscodeMed;

        let effects = super::preload_effects(&mut state);
        let (ids, profile) = prefetch_run(&effects).expect("a read-ahead run");

        let expected: Vec<crate::model::ItemId> = state.queue.play_order[4..7]
            .iter()
            .map(|&i| state.queue.entries[i].track.id.clone())
            .collect();
        assert_eq!(ids, expected);
        assert_eq!(
            profile,
            crate::config::QualityProfile::TranscodeMed,
            "a read-ahead at the wrong profile caches a file playback will never use"
        );
    }

    /// Three ways it must stay silent, each for its own reason: nothing to fetch *from* while
    /// offline, nowhere to put it with the cache off, and nothing asked for at zero.
    #[test]
    fn prefetch_is_skipped_when_it_could_not_help() {
        let base = || {
            let mut state = fixtures::fixture_playing_queue();
            state.config.cache.prefetch_next = 3;
            state
        };

        let mut off = base();
        off.config.cache.enabled = false;
        assert!(prefetch_run(&super::preload_effects(&mut off)).is_none());

        let mut zero = base();
        zero.config.cache.prefetch_next = 0;
        assert!(prefetch_run(&super::preload_effects(&mut zero)).is_none());

        let mut offline = base();
        offline.connectivity = Connectivity::Offline;
        assert!(prefetch_run(&super::preload_effects(&mut offline)).is_none());
    }

    /// Only remote entries are worth fetching — a cached or downloaded one is already local, and
    /// an unavailable one cannot be fetched at all.
    #[test]
    fn prefetch_skips_entries_that_are_already_local() {
        let mut state = fixtures::fixture_playing_queue();
        state.config.cache.prefetch_next = 2;
        let next = state.queue.play_order[4];
        state.queue.entries[next].availability = Availability::Downloaded;

        let effects = super::preload_effects(&mut state);
        let (ids, _) = prefetch_run(&effects).expect("a read-ahead run");

        let downloaded = state.queue.entries[next].track.id.clone();
        assert!(!ids.contains(&downloaded), "already on disk: {ids:?}");
        assert_eq!(ids.len(), 2, "the run should still reach two remote tracks");
    }

    /// At the end of the queue the tracks "ahead" under `Repeat::All` are ones already played, so
    /// the run does not wrap — unlike `preload_target`, which does.
    #[test]
    fn prefetch_does_not_wrap_at_the_end_of_the_queue() {
        let mut state = fixtures::fixture_playing_queue();
        state.config.cache.prefetch_next = 3;
        state.queue.repeat = RepeatMode::All;
        state.queue.position = state.queue.play_order.len() - 1;

        let effects = super::preload_effects(&mut state);
        assert!(prefetch_run(&effects).is_none());
    }

    #[test]
    fn toggle_shuffle_twice_unshuffles() {
        let mut state = playing_three_track_queue();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::ToggleShuffle { seed: 7 }),
        );
        assert!(state.queue.shuffled);
        dispatch(
            &mut state,
            Action::Queue(QueueAction::ToggleShuffle { seed: 7 }),
        );
        assert!(!state.queue.shuffled);
        assert_eq!(state.queue.play_order, vec![0, 1, 2]);
    }

    // --- 06-04: sort profiles ----------------------------------------------------------------

    fn push_alpha_by_name_profile(state: &mut AppState) {
        state
            .config
            .sorting
            .profiles
            .push(crate::config::SortProfile {
                name: "by_name".to_string(),
                rules: vec![crate::config::SortRule {
                    field: crate::config::SortField::Name,
                    direction: crate::config::Direction::Asc,
                }],
            });
    }

    #[test]
    fn apply_sort_profile_keeps_current_track_playing() {
        // Append order (cursor 0, 1, 2 in `tracks_state()`) is Motion, Fate, Come Closer —
        // alphabetically that's Come Closer, Fate, Motion, so sorting by name really does move
        // entries around, not just confirm an already-sorted no-op.
        let mut state = playing_three_track_queue();
        push_alpha_by_name_profile(&mut state);
        state.queue.position = 2; // "Come Closer" is current
        let current_id = state.queue.entries[state.queue.play_order[2]].entry_id;

        dispatch(
            &mut state,
            Action::Queue(QueueAction::ApplySortProfile("by_name".to_string())),
        );

        assert_eq!(
            state.queue.entries[state.queue.play_order[state.queue.position]].entry_id, current_id,
            "the same entry must still be current after the reorder"
        );
        let names: Vec<&str> = state
            .queue
            .play_order
            .iter()
            .map(|&i| state.queue.entries[i].track.name.as_str())
            .collect();
        assert_eq!(names, vec!["Come Closer", "Fate", "Motion"]);
    }

    #[test]
    fn apply_sort_profile_clears_shuffle() {
        let mut state = playing_three_track_queue();
        push_alpha_by_name_profile(&mut state);
        dispatch(
            &mut state,
            Action::Queue(QueueAction::ToggleShuffle { seed: 1 }),
        );
        assert!(state.queue.shuffled);

        dispatch(
            &mut state,
            Action::Queue(QueueAction::ApplySortProfile("by_name".to_string())),
        );
        assert!(!state.queue.shuffled);
    }

    #[test]
    fn apply_sort_profile_unknown_name_is_a_noop() {
        let mut state = playing_three_track_queue();
        let before = state.queue.clone();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::ApplySortProfile("does-not-exist".to_string())),
        );
        assert_eq!(state.queue, before);
    }

    // --- 06-06: gapless preloading -----------------------------------------------------------

    fn has_preload(effects: &[Effect]) -> bool {
        effects
            .iter()
            .any(|e| matches!(e, Effect::Audio(AudioEffect::Preload { .. })))
    }

    #[test]
    fn preload_target_respects_repeat_mode() {
        let mut q = fixtures::fixture_playing_queue().queue; // 10 entries, position = 3

        // Mid-queue: `Off`/`All` both target `position + 1`; `One` targets the current entry.
        q.repeat = RepeatMode::Off;
        assert_eq!(
            preload_target(&q).unwrap().entry_id,
            q.entries[q.play_order[4]].entry_id
        );
        q.repeat = RepeatMode::All;
        assert_eq!(
            preload_target(&q).unwrap().entry_id,
            q.entries[q.play_order[4]].entry_id
        );
        q.repeat = RepeatMode::One;
        assert_eq!(
            preload_target(&q).unwrap().entry_id,
            q.entries[q.play_order[3]].entry_id
        );

        // At the end: `Off` has nothing next, `All` wraps to index 0, `One` still targets self.
        q.position = q.play_order.len() - 1;
        q.repeat = RepeatMode::Off;
        assert!(preload_target(&q).is_none());
        q.repeat = RepeatMode::All;
        assert_eq!(
            preload_target(&q).unwrap().entry_id,
            q.entries[q.play_order[0]].entry_id
        );
        q.repeat = RepeatMode::One;
        assert_eq!(
            preload_target(&q).unwrap().entry_id,
            q.entries[q.play_order[q.play_order.len() - 1]].entry_id
        );
    }

    #[test]
    fn preload_target_none_at_end_with_repeat_off() {
        let mut q = fixtures::fixture_playing_queue().queue;
        q.position = q.play_order.len() - 1;
        q.repeat = RepeatMode::Off;
        assert!(preload_target(&q).is_none());
    }

    #[test]
    fn preload_emitted_after_queue_selection() {
        // Appending to a 1-entry queue creates a next entry where there wasn't one before.
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        state.active_column_mut().unwrap().cursor = 2;
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_insert_next() {
        // `InsertNext` directly replaces the next entry.
        let mut state = playing_three_track_queue();
        state.active_column_mut().unwrap().cursor = 2;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::InsertNext));
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_remove_entry() {
        // Removing the current (next-to-play) entry promotes the one after it.
        let mut state = playing_three_track_queue();
        let effects = dispatch(&mut state, Action::Queue(QueueAction::RemoveEntry));
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_move_entry() {
        // Reordering moves a different entry into the `position + 1` slot.
        let mut state = playing_three_track_queue();
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::MoveEntry { from: 2, to: 1 }),
        );
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_toggle_shuffle() {
        let mut state = playing_three_track_queue();
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::ToggleShuffle { seed: 1 }),
        );
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_apply_sort_profile() {
        // Alphabetically: Come Closer, Fate, Motion. Current on "Fate" (not the very end) so the
        // rebuild leaves a real `position + 1` target ("Motion") for it to preload.
        let mut state = playing_three_track_queue();
        push_alpha_by_name_profile(&mut state);
        state.queue.position = 1; // "Fate"
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::ApplySortProfile("by_name".to_string())),
        );
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_cycle_repeat() {
        // At the end of the queue, `Off` has no target; cycling to `All` gives it one (wrapping
        // to index 0), which is a fresh target for `CycleRepeat` to preload.
        let mut state = playing_three_track_queue();
        state.queue.position = state.queue.play_order.len() - 1;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::CycleRepeat));
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_jump_to() {
        // Setup left `position = 0` with `position + 1` (the 2nd entry) already preloaded.
        // Jumping to the 2nd entry moves `position + 1` on to the 3rd, a fresh target.
        let mut state = playing_three_track_queue();
        let target_id = state.queue.entries[state.queue.play_order[1]].entry_id;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::JumpTo(target_id)));
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_next() {
        let mut state = playing_three_track_queue();
        let effects = dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Next),
        );
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_prev() {
        let mut state = playing_three_track_queue();
        state.queue.position = 2;
        state.player.position = std::time::Duration::from_secs(1); // < 3s: moves back, not restart
        let effects = dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Prev),
        );
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_emitted_after_auto_advance() {
        let mut state = playing_three_track_queue();
        let effects = dispatch(
            &mut state,
            Action::Audio(crate::action::AudioEvent::TrackEnded { natural: true }),
        );
        assert!(has_preload(&effects));
    }

    #[test]
    fn preload_deduplicated_when_target_unchanged() {
        let mut state = tracks_state();
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        state.active_column_mut().unwrap().cursor = 0;
        dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        // The above already preloaded whatever landed at `position + 1`. Appending a third track
        // at the *end* doesn't touch that slot, so nothing new should be emitted.
        state.active_column_mut().unwrap().cursor = 2;
        let effects = dispatch(
            &mut state,
            Action::Queue(QueueAction::QueueSelection {
                full_context: false,
            }),
        );
        assert!(!has_preload(&effects));
    }

    #[test]
    fn no_preload_for_unavailable_entry() {
        let mut state = playing_three_track_queue();
        let next_index = state.queue.play_order[1];
        state.queue.entries[next_index].availability = Availability::Unavailable;
        state.player.last_preloaded = None;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::CycleRepeat));
        assert!(!has_preload(&effects));
    }

    #[test]
    fn no_preload_when_offline_and_remote() {
        let mut state = playing_three_track_queue();
        let next_index = state.queue.play_order[1];
        state.queue.entries[next_index].availability = Availability::Remote;
        state.connectivity = Connectivity::Offline;
        state.player.last_preloaded = None;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::CycleRepeat));
        assert!(!has_preload(&effects));
    }

    // --- 06-07: playback reporting wiring (queue-mechanics half) ------------------------------

    #[test]
    fn stopped_reported_on_clear() {
        let mut state = playing_three_track_queue();
        assert!(
            state.player.session.is_some(),
            "setup must have loaded a track"
        );
        let effects = dispatch(&mut state, Action::Queue(QueueAction::Clear));
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Net(NetEffect::ReportPlayback(
                crate::model::PlaybackReport::Stopped { .. }
            ))
        )));
        // `10-11`: `clear` sets `status` directly, bypassing `apply_audio`'s own `StatusChanged`
        // handler (`mpris_meta`'s other call site) — so it must call `mpris_meta` itself, or OS
        // media controls would keep showing the just-cleared track as still playing.
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Sys(SysEffect::UpdateMpris(meta)) if !meta.playing
        )));
    }

    #[test]
    fn session_id_stable_for_a_track() {
        let mut state = playing_three_track_queue();
        let session_after_load = state.player.session.clone();
        // A mutation that doesn't reload the current entry must not touch the session.
        dispatch(&mut state, Action::Queue(QueueAction::CycleRepeat));
        assert_eq!(state.player.session, session_after_load);
    }

    #[test]
    fn session_id_changes_between_tracks() {
        let mut state = playing_three_track_queue();
        let first_session = state.player.session.clone();
        dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Next),
        );
        assert_ne!(state.player.session, first_session);
    }

    #[test]
    fn played_flag_resets_on_new_load() {
        let mut state = playing_three_track_queue();
        state.player.play_reported = true; // simulate an already-reported play
        dispatch(
            &mut state,
            Action::Player(crate::action::PlayerAction::Next),
        );
        assert!(
            !state.player.play_reported,
            "a fresh Load must reset the Played guard"
        );
    }

    // --- 06-08: instant mix --------------------------------------------------------------------

    fn single_item_column(kind: ColumnKind, item: MediaItem) -> AppState {
        let mut col = Column::new(kind, "col");
        col.items = vec![item];
        col.cursor = 0;
        let mut state = fixtures::fixture_empty();
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![col]);
        focus_column(state, Tab::Artists, 0)
    }

    #[test]
    fn instant_mix_emits_effect_with_selected_id() {
        let a = fixtures::artist("Sync24");
        let mut state = single_item_column(ColumnKind::Artists, MediaItem::Artist(a.clone()));
        let effects = dispatch(&mut state, Action::Queue(QueueAction::InstantMix));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::InstantMix { seed, limit: 100 })] if *seed == a.id
        ));
    }

    #[test]
    fn instant_mix_works_for_every_item_type() {
        let a = fixtures::artist("Sync24");
        let alb = fixtures::album("Comfortable Void", 2012, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        let genre = MediaItem::Genre(crate::model::Genre {
            id: ItemId::from("genre-1"),
            name: "Darkwave".to_string(),
        });
        let playlist = MediaItem::Playlist(crate::model::Playlist {
            id: ItemId::from("playlist-1"),
            name: "Favourites".to_string(),
            overview: None,
            track_count: 0,
            total_duration: std::time::Duration::ZERO,
            can_edit: true,
            is_favorite: false,
        });

        let cases: Vec<(ColumnKind, MediaItem, ItemId)> = vec![
            (
                ColumnKind::Artists,
                MediaItem::Artist(a.clone()),
                a.id.clone(),
            ),
            (
                ColumnKind::Albums { of_artist: None },
                MediaItem::Album(alb.clone()),
                alb.id.clone(),
            ),
            (
                ColumnKind::Tracks {
                    of_album: alb.id.clone(),
                },
                MediaItem::Track(t.clone()),
                t.id.clone(),
            ),
            (ColumnKind::Genres, genre.clone(), ItemId::from("genre-1")),
            (
                ColumnKind::Playlists,
                playlist.clone(),
                ItemId::from("playlist-1"),
            ),
        ];

        for (kind, item, expected_id) in cases {
            let mut state = single_item_column(kind, item);
            let effects = dispatch(&mut state, Action::Queue(QueueAction::InstantMix));
            assert!(
                matches!(
                    effects.as_slice(),
                    [Effect::Net(NetEffect::InstantMix { seed, .. })] if *seed == expected_id
                ),
                "expected an InstantMix effect seeded from {expected_id}"
            );
        }
    }

    fn instant_mix_tracks(n: usize) -> Vec<Track> {
        let a = fixtures::artist("Mix Artist");
        let alb = fixtures::album("Mix Album", 2020, &a);
        (0..n)
            .map(|i| fixtures::track(&format!("Mix Track {i}"), i as u32, &alb, &[&a]))
            .collect()
    }

    #[test]
    fn instant_mix_replaces_queue() {
        let mut state = playing_three_track_queue();
        let seed = ItemId::from("seed-1");
        state.pending_instant_mix = Some("Sync24".to_string());
        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks: instant_mix_tracks(5),
                source: QueueSource::InstantMix { seed },
                full_context: false,
            }),
        );
        assert_eq!(state.queue.entries.len(), 5);
        assert!(
            state
                .queue
                .entries
                .iter()
                .all(|e| e.track.name.starts_with("Mix Track"))
        );
    }

    #[test]
    fn instant_mix_starts_playback_from_zero() {
        let mut state = playing_three_track_queue();
        state.queue.position = 2;
        state.pending_instant_mix = Some("Sync24".to_string());
        let effects = dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks: instant_mix_tracks(5),
                source: QueueSource::InstantMix {
                    seed: ItemId::from("seed-1"),
                },
                full_context: false,
            }),
        );
        assert_eq!(state.queue.position, 0);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cache(_),
                Effect::Audio(AudioEffect::Load { .. }),
                Effect::Sys(SysEffect::Notify(_)),
                Effect::Sys(SysEffect::UpdateMpris(_))
            ]
        ));
    }

    #[test]
    fn instant_mix_entries_carry_source_badge() {
        let mut state = fixtures::fixture_empty();
        let seed = ItemId::from("seed-1");
        state.pending_instant_mix = Some("Sync24".to_string());
        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks: instant_mix_tracks(3),
                source: QueueSource::InstantMix { seed: seed.clone() },
                full_context: false,
            }),
        );
        assert!(
            state
                .queue
                .entries
                .iter()
                .all(|e| e.source == QueueSource::InstantMix { seed: seed.clone() })
        );
    }

    #[test]
    fn instant_mix_does_not_apply_sort_profile() {
        let mut state = fixtures::fixture_empty();
        state.queue.sort_profile = Some("chronological_discog".to_string());
        let seed = ItemId::from("seed-1");
        state.pending_instant_mix = Some("Sync24".to_string());
        // Reply order is deliberately not disc/track sorted — an appears-on-style fetch would
        // re-sort it (`06-02`); an instant mix must not.
        let mut tracks = instant_mix_tracks(5);
        tracks.reverse();
        let expected: Vec<ItemId> = tracks.iter().map(|t| t.id.clone()).collect();
        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks,
                source: QueueSource::InstantMix { seed },
                full_context: false,
            }),
        );
        let actual: Vec<ItemId> = state
            .queue
            .play_order
            .iter()
            .map(|&i| state.queue.entries[i].track.id.clone())
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_instant_mix_leaves_queue_intact() {
        let mut state = playing_three_track_queue();
        let before = state.queue.clone();
        state.pending_instant_mix = Some("Sync24".to_string());
        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks: Vec::new(),
                source: QueueSource::InstantMix {
                    seed: ItemId::from("seed-1"),
                },
                full_context: false,
            }),
        );
        assert_eq!(state.queue, before);
    }

    #[test]
    fn empty_instant_mix_toasts() {
        let mut state = fixtures::fixture_empty();
        state.pending_instant_mix = Some("Sync24".to_string());
        dispatch(
            &mut state,
            Action::Data(DataAction::TracksLoaded {
                tracks: Vec::new(),
                source: QueueSource::InstantMix {
                    seed: ItemId::from("seed-1"),
                },
                full_context: false,
            }),
        );
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("no instant mix available for Sync24"))
        );
    }

    #[test]
    fn instant_mix_refused_when_offline() {
        let a = fixtures::artist("Sync24");
        let mut state = single_item_column(ColumnKind::Artists, MediaItem::Artist(a));
        state.connectivity = Connectivity::Offline;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::InstantMix));
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("instant mix requires a connection"))
        );
    }

    #[test]
    fn multiselect_seeds_from_first_and_toasts() {
        let a1 = fixtures::artist("Boy Harsher");
        let a2 = fixtures::artist("Sync24");
        let mut col = Column::new(ColumnKind::Artists, "Artists");
        col.items = vec![MediaItem::Artist(a1.clone()), MediaItem::Artist(a2.clone())];
        col.selection.visual_mode = true;
        col.selection.selected = [a1.id.clone(), a2.id.clone()].into_iter().collect();
        let mut state = fixtures::fixture_empty();
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![col]);
        let mut state = focus_column(state, Tab::Artists, 0);

        let effects = dispatch(&mut state, Action::Queue(QueueAction::InstantMix));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::InstantMix { seed, .. })] if *seed == a1.id
        ));
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("instant mix seeded from"))
        );
    }

    // --- 07-02: favourites tab (ToggleFavorite) -----------------------------------------------

    use crate::action::ItemAction;
    use crate::state::search::SearchResults;

    fn favourites_state_with(results: SearchResults) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.nav.active_tab = Tab::Favourites;
        state.favourites.results = results;
        state
    }

    #[test]
    fn toggle_favorite_updates_optimistically() {
        // A Miller-column track, not favourited yet.
        let mut state = tracks_state();
        let track_id = track_ids(&state)[1].clone(); // cursor is on index 1 ("Fate")
        assert!(!matches!(
            state.active_column().unwrap().items[1],
            MediaItem::Track(ref t) if t.is_favorite
        ));

        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::SetFavorite { id, on: true })] if *id == track_id
        ));
        assert!(matches!(
            state.active_column().unwrap().items[1],
            MediaItem::Track(ref t) if t.is_favorite
        ));
    }

    #[test]
    fn unfavourite_removes_row_immediately() {
        let a = fixtures::artist("Sync24");
        let alb = fixtures::album("Void", 2012, &a);
        let mut t = fixtures::track("Motion", 1, &alb, &[&a]);
        t.is_favorite = true;
        let mut state = favourites_state_with(SearchResults {
            tracks: vec![t.clone()],
            ..SearchResults::default()
        });
        state.favourites.focused_section = SearchSection::Tracks;

        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::SetFavorite { id, on: false })] if *id == t.id
        ));
        assert!(state.favourites.results.tracks.is_empty());
    }

    #[test]
    fn failure_restores_flag_and_row_at_original_index() {
        let a = fixtures::artist("Sync24");
        let alb = fixtures::album("Void", 2012, &a);
        let mut t1 = fixtures::track("Motion", 1, &alb, &[&a]);
        t1.is_favorite = true;
        let mut t2 = fixtures::track("Fate", 2, &alb, &[&a]);
        t2.is_favorite = true;
        let mut t3 = fixtures::track("Come Closer", 3, &alb, &[&a]);
        t3.is_favorite = true;
        let mut state = favourites_state_with(SearchResults {
            tracks: vec![t1, t2.clone(), t3],
            ..SearchResults::default()
        });
        state.favourites.focused_section = SearchSection::Tracks;
        state.favourites.cursors[SearchSection::Tracks.index()] = 1; // "Fate", the middle row

        dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert_eq!(
            state.favourites.results.tracks.len(),
            2,
            "removed optimistically"
        );

        dispatch(
            &mut state,
            Action::Data(DataAction::LoadFailed {
                target: crate::action::LoadTarget::FavoriteToggle(t2.id.clone()),
                message: "server error".to_string(),
                offline: false,
            }),
        );

        assert_eq!(state.favourites.results.tracks.len(), 3, "reinserted");
        assert_eq!(
            state.favourites.results.tracks[1].id, t2.id,
            "reinserted at its original index, not appended"
        );
        assert!(
            state.favourites.results.tracks[1].is_favorite,
            "flag restored"
        );
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("could not update favourite"))
        );
    }

    #[test]
    fn favorite_flag_updated_in_all_views() {
        let a = fixtures::artist("Sync24");
        let alb = fixtures::album("Void", 2012, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);

        // The same track present in a Miller column, the queue, and favourites.
        let mut state = tracks_state();
        state.active_column_mut().unwrap().items[0] = MediaItem::Track(t.clone());
        state.active_column_mut().unwrap().cursor = 0;
        let entry_id = state.queue.next_id();
        state.queue.entries.push(QueueEntry {
            entry_id,
            track: t.clone(),
            source: QueueSource::Manual,
            availability: Availability::Remote,
        });
        state.favourites.results.tracks.push(t.clone());

        dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));

        assert!(matches!(
            state.active_column().unwrap().items[0],
            MediaItem::Track(ref t) if t.is_favorite
        ));
        assert!(state.queue.entries[0].track.is_favorite);
        assert!(state.favourites.results.tracks[0].is_favorite);
    }

    #[test]
    fn favorite_refused_when_offline() {
        let mut state = tracks_state();
        state.connectivity = Connectivity::Offline;
        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("favourites need a connection"))
        );
        assert!(
            !matches!(
                state.active_column().unwrap().items[1],
                MediaItem::Track(ref t) if t.is_favorite
            ),
            "must not flip optimistically while refused"
        );
    }

    #[test]
    fn ctrl_r_refreshes() {
        let mut state = favourites_state_with(SearchResults::default());
        state.favourites.load = crate::state::nav::LoadState::Loaded { total: 0 };
        let effects = dispatch(
            &mut state,
            Action::System(crate::action::SystemEvent::Refresh),
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::FetchFavourites)]
        ));
        assert_eq!(state.favourites.load, crate::state::nav::LoadState::Loading);
    }

    // --- 07-03: playlists tab -------------------------------------------------------------

    fn playlist_item(id: &str, name: &str, track_count: u32) -> MediaItem {
        MediaItem::Playlist(crate::model::Playlist {
            id: ItemId::from(id),
            name: name.to_string(),
            overview: None,
            track_count,
            total_duration: std::time::Duration::ZERO,
            can_edit: true,
            is_favorite: false,
        })
    }

    /// A `Tab::Playlists` stack with just the `Playlists` list column loaded, focused.
    fn playlists_state(playlists: &[(&str, &str, u32)]) -> AppState {
        let mut state = fixtures::fixture_empty();
        let mut col = Column::new(ColumnKind::Playlists, "Playlists");
        col.items = playlists
            .iter()
            .map(|(id, name, count)| playlist_item(id, name, *count))
            .collect();
        state.nav.per_tab_stacks.insert(Tab::Playlists, vec![col]);
        state.nav.active_tab = Tab::Playlists;
        state.nav.focus = NavFocus::Column(0);
        state
    }

    /// The above, plus a drilled-in, focused `PlaylistTracks` column holding `tracks`.
    fn playlist_tracks_state(playlist_id: &str, tracks: Vec<Track>) -> AppState {
        let mut state = playlists_state(&[(playlist_id, "My Playlist", tracks.len() as u32)]);
        let mut col = Column::new(
            ColumnKind::PlaylistTracks {
                of_playlist: ItemId::from(playlist_id),
            },
            "My Playlist",
        );
        col.items = tracks.into_iter().map(MediaItem::Track).collect();
        state
            .nav
            .per_tab_stacks
            .get_mut(&Tab::Playlists)
            .unwrap()
            .push(col);
        state.nav.focus = NavFocus::Column(1);
        state
    }

    #[test]
    fn playlist_tracks_carry_entry_ids() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let t2 = fixtures::playlist_track("Fate", 2, &alb, &[&a], "entry-2");
        let state = playlist_tracks_state("playlist-1", vec![t1, t2]);

        let column = &state.nav.per_tab_stacks[&Tab::Playlists][1];
        let entry_ids: Vec<Option<PlaylistEntryId>> = column
            .items
            .iter()
            .map(|item| match item {
                MediaItem::Track(t) => t.playlist_entry_id.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(
            entry_ids,
            vec![
                Some(PlaylistEntryId::from("entry-1")),
                Some(PlaylistEntryId::from("entry-2")),
            ]
        );
    }

    #[test]
    fn duplicate_track_has_distinct_entry_ids() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        // Same underlying track (same album+track-number, hence the same `ItemId`) appearing
        // twice in the playlist — a real Emby shape, not a test artifact.
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let t2 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-2");
        assert_eq!(t1.id, t2.id, "the two rows share the same underlying track");
        assert_ne!(
            t1.playlist_entry_id, t2.playlist_entry_id,
            "but each row is a distinct playlist entry"
        );
    }

    #[test]
    fn remove_uses_entry_id_not_item_id() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let t2 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-2");
        assert_eq!(t1.id, t2.id);
        let mut state = playlist_tracks_state("playlist-1", vec![t1, t2]);

        let effects = dispatch(
            &mut state,
            Action::Item(ItemAction::RemoveFromPlaylist {
                playlist: PlaylistId::from("playlist-1"),
                entries: vec![PlaylistEntryId::from("entry-1")],
            }),
        );
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::PlaylistRemove { entries, .. })]
                if entries.as_slice() == [PlaylistEntryId::from("entry-1")]
        ));

        let column = &state.nav.per_tab_stacks[&Tab::Playlists][1];
        assert_eq!(column.items.len(), 1, "only the matching entry is removed");
        assert!(matches!(
            &column.items[0],
            MediaItem::Track(t) if t.playlist_entry_id == Some(PlaylistEntryId::from("entry-2"))
        ));
    }

    #[test]
    fn remove_is_optimistic_and_rolls_back() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let t2 = fixtures::playlist_track("Fate", 2, &alb, &[&a], "entry-2");
        let mut state = playlist_tracks_state("playlist-1", vec![t1, t2]);

        dispatch(
            &mut state,
            Action::Item(ItemAction::RemoveFromPlaylist {
                playlist: PlaylistId::from("playlist-1"),
                entries: vec![PlaylistEntryId::from("entry-1")],
            }),
        );
        assert_eq!(
            state.nav.per_tab_stacks[&Tab::Playlists][1].items.len(),
            1,
            "removed immediately, before the network replies"
        );

        dispatch(
            &mut state,
            Action::Data(DataAction::LoadFailed {
                target: crate::action::LoadTarget::PlaylistMutation(PlaylistId::from("playlist-1")),
                message: "server error".to_string(),
                offline: false,
            }),
        );

        let column = &state.nav.per_tab_stacks[&Tab::Playlists][1];
        assert_eq!(column.items.len(), 2, "reinserted after the failure");
        assert!(
            matches!(
                &column.items[0],
                MediaItem::Track(t) if t.playlist_entry_id == Some(PlaylistEntryId::from("entry-1"))
            ),
            "reinserted at its original index, not appended"
        );
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("playlist change failed"))
        );
    }

    #[test]
    fn reorder_rolls_back_to_original_index() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let t2 = fixtures::playlist_track("Fate", 2, &alb, &[&a], "entry-2");
        let t3 = fixtures::playlist_track("Come Closer", 3, &alb, &[&a], "entry-3");
        let mut state = playlist_tracks_state("playlist-1", vec![t1, t2, t3]);

        dispatch(
            &mut state,
            Action::Item(ItemAction::MoveInPlaylist {
                playlist: PlaylistId::from("playlist-1"),
                entry: PlaylistEntryId::from("entry-1"),
                new_index: 2,
            }),
        );
        let moved = &state.nav.per_tab_stacks[&Tab::Playlists][1];
        assert!(matches!(
            &moved.items[2],
            MediaItem::Track(t) if t.playlist_entry_id == Some(PlaylistEntryId::from("entry-1"))
        ));

        dispatch(
            &mut state,
            Action::Data(DataAction::LoadFailed {
                target: crate::action::LoadTarget::PlaylistMutation(PlaylistId::from("playlist-1")),
                message: "server error".to_string(),
                offline: false,
            }),
        );

        let restored = &state.nav.per_tab_stacks[&Tab::Playlists][1];
        assert!(
            matches!(
                &restored.items[0],
                MediaItem::Track(t) if t.playlist_entry_id == Some(PlaylistEntryId::from("entry-1"))
            ),
            "moved back to its original index (0), not left at the failed destination"
        );
    }

    #[test]
    fn delete_requires_confirmation() {
        let mut state = playlists_state(&[("playlist-1", "Late Night Drives", 12)]);

        let effects = dispatch(
            &mut state,
            Action::Item(ItemAction::DeletePlaylist(PlaylistId::from("playlist-1"))),
        );
        assert!(effects.is_empty(), "no network effect until confirmed");
        assert!(
            matches!(
                &state.modal,
                Some(crate::state::modal::Modal::Confirm { .. })
            ),
            "opens a Confirm modal instead of deleting directly"
        );
        assert_eq!(
            state.nav.per_tab_stacks[&Tab::Playlists][0].items.len(),
            1,
            "playlist row untouched until the Confirm is actually accepted"
        );
    }

    #[test]
    fn confirm_prompt_names_playlist_and_count() {
        let mut state = playlists_state(&[("playlist-1", "Late Night Drives", 12)]);
        dispatch(
            &mut state,
            Action::Item(ItemAction::DeletePlaylist(PlaylistId::from("playlist-1"))),
        );
        let Some(crate::state::modal::Modal::Confirm { prompt, on_confirm }) = &state.modal else {
            panic!("expected a Confirm modal");
        };
        assert!(prompt.contains("Late Night Drives"));
        assert!(prompt.contains("12"));
        assert!(matches!(
            on_confirm.as_ref(),
            Action::Item(ItemAction::DeletePlaylistConfirmed(id)) if *id == PlaylistId::from("playlist-1")
        ));
    }

    #[test]
    fn multiselect_remove_sends_one_request() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let t2 = fixtures::playlist_track("Fate", 2, &alb, &[&a], "entry-2");
        let t3 = fixtures::playlist_track("Come Closer", 3, &alb, &[&a], "entry-3");
        let mut state = playlist_tracks_state("playlist-1", vec![t1, t2, t3]);

        // Two rows selected (as `v` + `.` would leave `column.selection`), removed together.
        let effects = dispatch(
            &mut state,
            Action::Item(ItemAction::RemoveFromPlaylist {
                playlist: PlaylistId::from("playlist-1"),
                entries: vec![
                    PlaylistEntryId::from("entry-1"),
                    PlaylistEntryId::from("entry-3"),
                ],
            }),
        );
        assert_eq!(effects.len(), 1, "one request for both rows, not two");
        assert!(matches!(
            effects.as_slice(),
            [Effect::Net(NetEffect::PlaylistRemove { entries, .. })] if entries.len() == 2
        ));
        assert_eq!(state.nav.per_tab_stacks[&Tab::Playlists][1].items.len(), 1);
    }

    #[test]
    fn mutations_refused_when_offline() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let mut state = playlist_tracks_state("playlist-1", vec![t1]);
        state.connectivity = Connectivity::Offline;

        let remove_effects = dispatch(
            &mut state,
            Action::Item(ItemAction::RemoveFromPlaylist {
                playlist: PlaylistId::from("playlist-1"),
                entries: vec![PlaylistEntryId::from("entry-1")],
            }),
        );
        assert!(remove_effects.is_empty());
        assert_eq!(state.nav.per_tab_stacks[&Tab::Playlists][1].items.len(), 1);

        let move_effects = dispatch(
            &mut state,
            Action::Item(ItemAction::MoveInPlaylist {
                playlist: PlaylistId::from("playlist-1"),
                entry: PlaylistEntryId::from("entry-1"),
                new_index: 0,
            }),
        );
        assert!(move_effects.is_empty());

        let delete_effects = dispatch(
            &mut state,
            Action::Item(ItemAction::DeletePlaylistConfirmed(PlaylistId::from(
                "playlist-1",
            ))),
        );
        assert!(delete_effects.is_empty());
        assert_eq!(state.nav.per_tab_stacks[&Tab::Playlists][0].items.len(), 1);
        // `10-13`: all three refusals toast the exact same message — deduplicated down to one
        // toast (refreshed each time), not stacked three deep.
        assert_eq!(
            state
                .toasts
                .iter()
                .filter(|t| t.message.contains("playlist changes need a connection"))
                .count(),
            1
        );
    }

    /// `08-06`'s own named acceptance test — a table over every reducer entry point
    /// `docs/06-cache-and-offline.md` §6 names as needing a connection: favourite toggle,
    /// playlist add (via the `SavePlaylist` modal's own submit path,
    /// `reducer::modal::submit`), playlist delete, and instant mix. Each already has its own
    /// guard (`toggle_favorite`/`delete_playlist`/`instant_mix`, plus the `Modal::SavePlaylist`
    /// arm this same task added) — this is the cross-cutting inventory proving all four still
    /// refuse and toast a specific message, in one place.
    #[test]
    fn offline_refuses_mutations_with_specific_toasts() {
        // Favourite toggle.
        let mut state = tracks_state();
        state.connectivity = Connectivity::Offline;
        let effects = dispatch(&mut state, Action::Item(ItemAction::ToggleFavorite));
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("favourites need a connection"))
        );

        // Instant mix.
        let mut state = tracks_state();
        state.connectivity = Connectivity::Offline;
        let effects = dispatch(&mut state, Action::Queue(QueueAction::InstantMix));
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("instant mix requires a connection"))
        );

        // Playlist add (`Ctrl+P`, an existing playlist) via the `SavePlaylist` modal's submit.
        let mut state = fixtures::fixture_empty();
        state.connectivity = Connectivity::Offline;
        state.modal = Some(crate::state::modal::Modal::SavePlaylist {
            target: crate::state::modal::PlaylistTarget::Existing(PlaylistId::from("playlist-1")),
            target_cursor: 1,
            name: String::new(),
            overview: String::new(),
            autosort: false,
            field: 0,
            source: crate::state::modal::SaveSource::Queue,
            error: None,
        });
        let effects = dispatch(&mut state, Action::Modal(ModalAction::Submit));
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("playlist changes need a connection"))
        );

        // Playlist delete.
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t1 = fixtures::playlist_track("Motion", 1, &alb, &[&a], "entry-1");
        let mut state = playlist_tracks_state("playlist-1", vec![t1]);
        state.connectivity = Connectivity::Offline;
        let effects = dispatch(
            &mut state,
            Action::Item(ItemAction::DeletePlaylistConfirmed(PlaylistId::from(
                "playlist-1",
            ))),
        );
        assert!(effects.is_empty());
        assert!(
            state
                .toasts
                .iter()
                .any(|t| t.message.contains("playlist changes need a connection"))
        );
    }

    proptest! {
        #[test]
        fn queue_invariants_hold_after_shuffle(ops in prop::collection::vec(0u8..9, 0..200)) {
            let mut state = tracks_state();
            for op in ops {
                match op {
                    0 => {
                        let cursor = state.active_column().map(|c| c.items.len()).unwrap_or(1);
                        if cursor > 0 {
                            state.active_column_mut().unwrap().cursor = 0;
                        }
                        dispatch(&mut state, Action::Queue(QueueAction::QueueSelection { full_context: false }));
                    }
                    1 => { dispatch(&mut state, Action::Queue(QueueAction::InsertNext)); }
                    2 => { dispatch(&mut state, Action::Queue(QueueAction::RemoveEntry)); }
                    3 => {
                        let len = state.queue.play_order.len();
                        if len > 1 {
                            dispatch(&mut state, Action::Queue(QueueAction::MoveEntry { from: 0, to: len - 1 }));
                        }
                    }
                    4 => { dispatch(&mut state, Action::Queue(QueueAction::CycleRepeat)); }
                    5 => {
                        dispatch(&mut state, Action::Audio(crate::action::AudioEvent::TrackEnded { natural: true }));
                    }
                    6 => { dispatch(&mut state, Action::Player(crate::action::PlayerAction::Next)); }
                    7 => { dispatch(&mut state, Action::Queue(QueueAction::ToggleShuffle { seed: 42 })); }
                    _ => { dispatch(&mut state, Action::Queue(QueueAction::ToggleShuffle { seed: 7 })); }
                }
                debug_assert_invariants(&state);
            }
        }
    }

    proptest! {
        #[test]
        fn queue_invariants_hold(ops in prop::collection::vec(0u8..7, 0..200)) {
            let mut state = tracks_state();
            for op in ops {
                match op {
                    0 => {
                        let cursor = state.active_column().map(|c| c.items.len()).unwrap_or(1);
                        if cursor > 0 {
                            state.active_column_mut().unwrap().cursor = 0;
                        }
                        dispatch(&mut state, Action::Queue(QueueAction::QueueSelection { full_context: false }));
                    }
                    1 => { dispatch(&mut state, Action::Queue(QueueAction::InsertNext)); }
                    2 => { dispatch(&mut state, Action::Queue(QueueAction::RemoveEntry)); }
                    3 => {
                        let len = state.queue.play_order.len();
                        if len > 1 {
                            dispatch(&mut state, Action::Queue(QueueAction::MoveEntry { from: 0, to: len - 1 }));
                        }
                    }
                    4 => { dispatch(&mut state, Action::Queue(QueueAction::CycleRepeat)); }
                    5 => {
                        dispatch(&mut state, Action::Audio(crate::action::AudioEvent::TrackEnded { natural: true }));
                    }
                    _ => { dispatch(&mut state, Action::Player(crate::action::PlayerAction::Next)); }
                }
                debug_assert_invariants(&state);
            }
        }
    }
}
