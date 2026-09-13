//! Deterministic builders and `AppState` fixtures. No randomness, no clock — every timestamp
//! comes from [`fixed_epoch`] or an offset of it, never `Timestamp::now()`, so every snapshot test
//! built on these stays reproducible.

use std::time::Duration;

use crate::Timestamp;
use crate::model::{
    Album, AlbumRelation, Artist, AudioFormat, Codec, Folder, Genre, ItemId, LyricFormat,
    LyricStreamRef, MediaItem, MediaSourceId, PlaylistEntryId, SectionHeader, SectionKind, Track,
};
use crate::state::AppState;
use crate::state::nav::{Column, ColumnKind, NavFocus, Tab};
use crate::state::player::PlayStatus;
use crate::state::queue::{Availability, HistoryEntry, QueueEntry, QueueSource};

/// A fixed instant (2023-11-14T22:13:20Z) every fixture timestamp is derived from.
pub fn fixed_epoch() -> Timestamp {
    Timestamp::from_second(1_700_000_000).expect("fixed epoch is a valid timestamp")
}

fn epoch_plus_secs(secs: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + secs).expect("fixed epoch offset is a valid timestamp")
}

pub fn artist(name: &str) -> Artist {
    Artist {
        id: ItemId::from(format!("artist-{name}")),
        name: name.to_string(),
        sort_name: name.to_string(),
        album_count: 0,
        track_count: 0,
        genres: Vec::new(),
        is_favorite: false,
        image: None,
        overview: None,
    }
}

pub fn genre(name: &str) -> Genre {
    Genre {
        id: ItemId::from(format!("genre-{name}")),
        name: name.to_string(),
    }
}

pub fn album(name: &str, year: u16, artist: &Artist) -> Album {
    Album {
        id: ItemId::from(format!("album-{name}")),
        name: name.to_string(),
        sort_name: name.to_string(),
        album_artist_names: vec![artist.name.clone()],
        album_artist_ids: vec![artist.id.clone()],
        year: Some(year),
        track_count: 0,
        total_duration: Duration::ZERO,
        genres: Vec::new(),
        is_favorite: false,
        image: None,
        relation: AlbumRelation::Primary,
    }
}

/// A compilation `context` merely appears on — the nominal album artist is a placeholder, never
/// `context` itself, matching the real Emby shape this fixture stands in for.
pub fn appears_on_album(name: &str, year: u16, context: &Artist) -> Album {
    Album {
        id: ItemId::from(format!("album-{name}")),
        name: name.to_string(),
        sort_name: name.to_string(),
        album_artist_names: vec!["Various Artists".to_string()],
        album_artist_ids: vec![ItemId::from("artist-various-artists")],
        year: Some(year),
        track_count: 0,
        total_duration: Duration::ZERO,
        genres: Vec::new(),
        is_favorite: false,
        image: None,
        relation: AlbumRelation::AppearsOn {
            context_artist: context.id.clone(),
        },
    }
}

pub fn track(name: &str, n: u32, album: &Album, artists: &[&Artist]) -> Track {
    Track {
        id: ItemId::from(format!("{}-track-{n}", album.id)),
        name: name.to_string(),
        album_id: Some(album.id.clone()),
        album_name: album.name.clone(),
        album_artist_names: album.album_artist_names.clone(),
        artist_ids: artists.iter().map(|a| a.id.clone()).collect(),
        artist_names: artists.iter().map(|a| a.name.clone()).collect(),
        track_number: Some(n),
        disc_number: Some(1),
        year: album.year,
        duration: Duration::from_secs(180),
        genres: Vec::new(),
        is_favorite: false,
        play_count: 0,
        format: AudioFormat {
            codec: Codec::Flac,
            sample_rate_hz: 44_100,
            bit_depth: Some(16),
            channels: 2,
            bitrate_bps: Some(1_000_000),
        },
        replay_gain: None,
        image: None,
        media_source_id: None,
        lyric_stream: None,
        date_created: None,
        playlist_entry_id: None,
    }
}

/// Same as [`track`], plus a discovered `.lrc` lyric stream.
pub fn track_with_lyrics(name: &str, n: u32, album: &Album, artists: &[&Artist]) -> Track {
    let mut t = track(name, n, album, artists);
    let media_source_id = MediaSourceId::from(format!("{}-ms", t.id));
    t.lyric_stream = Some(LyricStreamRef {
        media_source_id: media_source_id.clone(),
        stream_index: 2,
        format: LyricFormat::Lrc,
    });
    t.media_source_id = Some(media_source_id);
    t
}

/// Same as [`track`], but stamped with a `PlaylistEntryId` (`07-03`) — a row inside a
/// `ColumnKind::PlaylistTracks` column, as opposed to a track appearing anywhere else.
pub fn playlist_track(
    name: &str,
    n: u32,
    album: &Album,
    artists: &[&Artist],
    entry_id: &str,
) -> Track {
    let mut t = track(name, n, album, artists);
    t.playlist_entry_id = Some(PlaylistEntryId::from(entry_id));
    t
}

pub fn fixture_empty() -> AppState {
    AppState::default()
}

/// Artists → Albums → Tracks, cursor mid-list in the (rightmost, focused) third column.
pub fn fixture_miller_3col() -> AppState {
    let mut state = fixture_empty();

    let a1 = artist("Boy Harsher");
    let a2 = artist("Sync24");
    let alb1 = album("Care", 2019, &a1);
    let alb2 = album("Careful", 2017, &a1);
    let t1 = track("Motion", 1, &alb1, &[&a1]);
    let t2 = track("Fate", 2, &alb1, &[&a1]);
    let t3 = track("Come Closer", 3, &alb1, &[&a1]);

    let mut artists_col = Column::new(ColumnKind::Artists, "Artists");
    artists_col.items = vec![MediaItem::Artist(a1.clone()), MediaItem::Artist(a2)];

    let mut albums_col = Column::new(
        ColumnKind::Albums {
            of_artist: Some(a1.id.clone()),
        },
        "Albums",
    );
    albums_col.items = vec![MediaItem::Album(alb1.clone()), MediaItem::Album(alb2)];

    let mut tracks_col = Column::new(
        ColumnKind::Tracks {
            of_album: alb1.id.clone(),
        },
        "Tracks",
    );
    tracks_col.items = vec![
        MediaItem::Track(t1),
        MediaItem::Track(t2),
        MediaItem::Track(t3),
    ];
    tracks_col.cursor = 1;

    state.nav.active_tab = Tab::Artists;
    state
        .nav
        .per_tab_stacks
        .insert(Tab::Artists, vec![artists_col, albums_col, tracks_col]);
    state.nav.focus = NavFocus::Column(2);
    state
}

/// Five columns deep (nested folders), for sliding-window tests — only 3 columns are ever
/// visible, so `window_start` must slide as the focus moves past them.
pub fn fixture_miller_5col() -> AppState {
    let mut state = fixture_empty();

    let mut columns = Vec::new();
    for i in 0..5u32 {
        let parent = ItemId::from(format!("folder-{i}"));
        let mut col = Column::new(
            ColumnKind::Folders {
                of_parent: Some(parent),
            },
            format!("Folder {i}"),
        );
        col.items = vec![MediaItem::Folder(Folder {
            id: ItemId::from(format!("folder-{i}-child")),
            name: format!("Child {i}"),
        })];
        columns.push(col);
    }

    state.nav.active_tab = Tab::Folders;
    state.nav.per_tab_stacks.insert(Tab::Folders, columns);
    state.nav.focus = NavFocus::Column(4);
    state.nav.window_start = 2;
    state
}

/// One artist, 2 primary albums, 2 compilations shown in a single Albums column (the compilations'
/// own 10-tracks-each/2-featuring-the-artist shape is verified directly against the builders, not
/// through this fixture's `AppState` — see `appears_on_fixture_shape` in the test module).
pub fn fixture_appears_on() -> AppState {
    let mut state = fixture_empty();

    let main_artist = artist("Sync24");
    let primary1 = album("Comfortable Void", 2012, &main_artist);
    let primary2 = album("Source", 2007, &main_artist);
    let comp1 = appears_on_album("Fahrenheit Project, Part Five", 2005, &main_artist);
    let comp2 = appears_on_album("Fahrenheit Project, Part Six", 2006, &main_artist);

    let mut albums_col = Column::new(
        ColumnKind::Albums {
            of_artist: Some(main_artist.id.clone()),
        },
        "Albums",
    );
    albums_col.items = vec![
        MediaItem::Album(primary1),
        MediaItem::Album(primary2),
        MediaItem::SectionHeader(SectionHeader {
            label: "APPEARS ON".to_string(),
            count: 2,
            kind: SectionKind::AppearsOn,
        }),
        MediaItem::Album(comp1),
        MediaItem::Album(comp2),
    ];

    state.nav.active_tab = Tab::Artists;
    state
        .nav
        .per_tab_stacks
        .insert(Tab::Artists, vec![albums_col]);
    state
}

/// 3 of 5 tracks selected, visual mode on.
pub fn fixture_visual_select() -> AppState {
    let mut state = fixture_empty();

    let a = artist("Boy Harsher");
    let alb = album("Care", 2019, &a);
    let tracks: Vec<Track> = (1..=5)
        .map(|n| track(&format!("Track {n}"), n, &alb, &[&a]))
        .collect();

    let mut col = Column::new(
        ColumnKind::Tracks {
            of_album: alb.id.clone(),
        },
        "Tracks",
    );
    col.selection.visual_mode = true;
    col.selection.selected = tracks[0..3].iter().map(|t| t.id.clone()).collect();
    col.items = tracks.into_iter().map(MediaItem::Track).collect();

    state.nav.active_tab = Tab::Artists;
    state.nav.per_tab_stacks.insert(Tab::Artists, vec![col]);
    state.nav.focus = NavFocus::Column(0);
    state
}

/// A 10-entry queue, position 3, playing.
pub fn fixture_playing_queue() -> AppState {
    let mut state = fixture_empty();

    let a = artist("Boy Harsher");
    let alb = album("Care", 2019, &a);
    let mut entries = Vec::new();
    for n in 1..=10u32 {
        let t = track(&format!("Track {n}"), n, &alb, &[&a]);
        let entry_id = state.queue.next_id();
        entries.push(QueueEntry {
            entry_id,
            track: t,
            source: QueueSource::Album { id: alb.id.clone() },
            availability: Availability::Remote,
        });
    }

    state.queue.play_order = (0..entries.len()).collect();
    state.queue.entries = entries;
    state.queue.position = 3;
    state.player.status = PlayStatus::Playing;
    state.player.current = state.queue.current().map(|e| e.entry_id);
    // Already playing, so its `Sessions/Playing` has already gone out — a further `Playing` status
    // is a resume, reported as Progress (`PlayerState::start_reported`).
    state.player.start_reported = true;
    state
}

/// `Connectivity::Offline`, mixed availability across a playing queue.
pub fn fixture_offline() -> AppState {
    let mut state = fixture_playing_queue();
    state.connectivity = crate::state::Connectivity::Offline;
    for (i, entry) in state.queue.entries.iter_mut().enumerate() {
        entry.availability = match i % 3 {
            0 => Availability::Downloaded,
            1 => Availability::Cached,
            _ => Availability::Unavailable,
        };
    }
    state
}

/// 50 history entries at fixed, distinct timestamps (newest first).
pub fn fixture_with_history() -> AppState {
    let mut state = fixture_empty();

    let a = artist("Boy Harsher");
    let alb = album("Care", 2019, &a);
    for n in 0..50u32 {
        let t = track(&format!("Track {n}"), n, &alb, &[&a]);
        state.history.push_back(HistoryEntry {
            track: t,
            played_at: epoch_plus_secs(i64::from(n)),
            completed: true,
        });
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_fixtures() -> Vec<(&'static str, AppState)> {
        vec![
            ("empty", fixture_empty()),
            ("miller_3col", fixture_miller_3col()),
            ("miller_5col", fixture_miller_5col()),
            ("appears_on", fixture_appears_on()),
            ("visual_select", fixture_visual_select()),
            ("playing_queue", fixture_playing_queue()),
            ("offline", fixture_offline()),
            ("with_history", fixture_with_history()),
        ]
    }

    #[test]
    fn every_fixture_constructs() {
        for (name, state) in all_fixtures() {
            if let Some(column) = state.active_column() {
                assert!(
                    column.cursor <= column.items.len(),
                    "{name}: cursor out of range"
                );
            }
            if !state.queue.entries.is_empty() {
                assert!(
                    state.queue.position < state.queue.play_order.len(),
                    "{name}: queue position out of range"
                );
            }
        }
    }

    #[test]
    fn fixtures_are_deterministic() {
        assert_eq!(fixture_miller_3col(), fixture_miller_3col());
        assert_eq!(fixture_appears_on(), fixture_appears_on());
        assert_eq!(fixture_playing_queue(), fixture_playing_queue());
        assert_eq!(fixture_with_history(), fixture_with_history());
    }

    #[test]
    fn fixtures_contain_no_current_timestamps() {
        let state = fixture_with_history();
        for entry in &state.history {
            assert!(
                entry.played_at >= fixed_epoch(),
                "history timestamp predates the fixed epoch"
            );
            assert!(
                entry.played_at <= epoch_plus_secs(49),
                "history timestamp is not derived from the fixed epoch"
            );
        }
    }

    #[test]
    fn appears_on_fixture_shape() {
        let state = fixture_appears_on();
        let column = &state.nav.per_tab_stacks[&Tab::Artists][0];
        let albums: Vec<&Album> = column
            .items
            .iter()
            .filter_map(|i| {
                if let MediaItem::Album(a) = i {
                    Some(a)
                } else {
                    None
                }
            })
            .collect();
        let primary_count = albums
            .iter()
            .filter(|a| a.relation == AlbumRelation::Primary)
            .count();
        let appears_on_count = albums
            .iter()
            .filter(|a| !matches!(a.relation, AlbumRelation::Primary))
            .count();
        assert_eq!(primary_count, 2);
        assert_eq!(appears_on_count, 2);

        // The compilation shape itself: 10 tracks, exactly 2 featuring the context artist.
        let main_artist = artist("Sync24");
        let other_artist = artist("Other Artist");
        let comp = appears_on_album("Fahrenheit Project, Part Five", 2005, &main_artist);
        let tracks: Vec<Track> = (0..10u32)
            .map(|i| {
                if i < 2 {
                    track(&format!("Track {i}"), i + 1, &comp, &[&main_artist])
                } else {
                    track(&format!("Track {i}"), i + 1, &comp, &[&other_artist])
                }
            })
            .collect();
        assert_eq!(tracks.len(), 10);
        assert_eq!(
            tracks
                .iter()
                .filter(|t| t.artist_ids.contains(&main_artist.id))
                .count(),
            2
        );
    }
}
