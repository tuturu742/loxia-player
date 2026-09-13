//! The rightmost metadata pane (`docs/07-ui-spec.md` §6): per-kind detail, the multi-select
//! summary, and the keymap-derived contextual action list. The inspector is not focusable in this
//! phase, so no scrolling is implemented for the overview text — only clipping plus an overflow
//! indicator.

use std::time::Duration;

use loxia_core::config::ReplayGainMode;
use loxia_core::keymap::ActionId;
use loxia_core::model::{Album, AlbumRelation, Artist, MediaItem, Track};
use loxia_core::state::AppState;
use loxia_core::state::nav::Tab;
use loxia_core::state::player::AppliedGain;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;
use crate::text;

/// Floor and ceiling on the art block at the top of the pane. A third of the inspector's height
/// between them: enough that a tall terminal shows a real image rather than a stamp, without the
/// artwork ever crowding out the metadata it sits above.
const MIN_ART_HEIGHT: u16 = 6;
const MAX_ART_HEIGHT: u16 = 14;

fn art_height(area: Rect) -> u16 {
    (area.height / 3)
        .clamp(MIN_ART_HEIGHT, MAX_ART_HEIGHT)
        .min(area.height)
}

pub fn render(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }

    let Some(col) = state.active_column() else {
        // No column, nothing to illustrate — but the art block still holds its space so the pane
        // doesn't reflow the instant a column appears.
        let art_height = art_height_for(state, area);
        render_art_placeholder(f, Rect::new(area.x, area.y, area.width, art_height), theme);
        render_dim_dash(
            f,
            Rect::new(
                area.x,
                area.y + art_height,
                area.width,
                area.height - art_height,
            ),
            theme,
        );
        return Vec::new();
    };

    let selected: Vec<MediaItem> = col
        .selection
        .selected
        .iter()
        .filter_map(|id| col.items.iter().find(|i| i.id() == Some(id)))
        .cloned()
        .collect();

    let focus_item: Option<MediaItem> = if selected.len() > 1 {
        None
    } else if selected.len() == 1 {
        Some(selected[0].clone())
    } else {
        col.items.get(col.cursor).cloned()
    };

    // Drawn from the item in hand, so the Artists and Album Artists lists show the artist's own
    // image and Albums its cover — "art on top of metadata", as asked. With the setting off no
    // space is reserved and no fetch is issued at all.
    let art_height = art_height_for(state, area);
    let mut effects = Vec::new();
    if art_height > 0 {
        let art_area = Rect::new(area.x, area.y, area.width, art_height);
        effects.extend(crate::widgets::album_art::render(
            f,
            crate::widgets::album_art::art_rect(
                crate::widgets::album_art::ArtSizeContext::Inspector,
                art_area,
            ),
            crate::widgets::album_art::ArtSizeContext::Inspector,
            art,
            focus_item.as_ref(),
            theme,
        ));
    }
    let rest = Rect::new(
        area.x,
        area.y + art_height,
        area.width,
        area.height - art_height,
    );

    let action_list = actions_for(
        focus_item.as_ref(),
        selected.len(),
        selected.len() > 1 && selected.iter().all(|i| matches!(i, MediaItem::Track(_))),
        state.nav.active_tab,
    );
    let footer_height = if action_list.is_empty() {
        0
    } else {
        (1 + action_list.len() as u16).min(rest.height)
    };
    let body_height = rest.height - footer_height;
    let body_area = Rect::new(rest.x, rest.y, rest.width, body_height);
    let footer_area = Rect::new(rest.x, rest.y + body_height, rest.width, footer_height);

    if selected.len() > 1 {
        render_multiselect_body(f, body_area, &selected, theme);
    } else if let Some(item) = &focus_item {
        match item {
            MediaItem::Artist(a) => render_artist_body(f, body_area, a, theme),
            MediaItem::Album(a) => render_album_body(f, body_area, a, theme),
            MediaItem::Track(t) => render_track_body(f, body_area, t, state, theme),
            MediaItem::Genre(_) | MediaItem::Folder(_) | MediaItem::Playlist(_) => {
                render_generic_body(f, body_area, item, theme)
            }
            MediaItem::SectionHeader(_) => render_dim_dash(f, body_area, theme),
        }
    } else {
        render_dim_dash(f, body_area, theme);
    }

    render_actions(
        f,
        footer_area,
        &action_list,
        focus_item.as_ref(),
        state,
        theme,
        hits,
    );
    effects
}

/// Zero when `ui.show_inspector_art` is off — the rows go back to the metadata, and nothing is
/// fetched, which is the whole point of the switch.
fn art_height_for(state: &AppState, area: Rect) -> u16 {
    if state.config.ui.show_inspector_art {
        art_height(area)
    } else {
        0
    }
}

fn render_art_placeholder(f: &mut Frame, area: Rect, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    // Reserved for task `10-01`'s real album-art rendering — a plain bordered box for now.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, false));
    f.render_widget(block, area);
}

fn render_dim_dash(f: &mut Frame, area: Rect, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    let row = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled("—", style::fg(theme, Role::Dim)))),
        row,
    );
}

fn draw_rows(f: &mut Frame, area: Rect, lines: &[Line<'static>]) {
    for (i, line) in lines.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }
        let row = Rect::new(area.x, area.y + i as u16, area.width, 1);
        f.render_widget(Paragraph::new(line.clone()), row);
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn render_multiselect_body(f: &mut Frame, area: Rect, selected: &[MediaItem], theme: &Theme) {
    let total: Duration = selected
        .iter()
        .filter_map(|i| match i {
            MediaItem::Track(t) => Some(t.duration),
            _ => None,
        })
        .sum();

    let fg = style::style(theme, Role::Fg);
    let mut lines = vec![Line::from(Span::styled(
        format!("{} {} Selected", selected.len(), selection_noun(selected)),
        fg,
    ))];
    // A running total only means anything when the rows carry durations. Selected albums do not
    // (a `MediaItem::Album` has no duration to sum), so the line would read a flat `Total: 0:00`.
    if !total.is_zero() {
        lines.push(Line::from(Span::styled(
            format!("Total: {}", format_duration(total)),
            fg,
        )));
    }
    draw_rows(f, area, &lines);
}

/// What a multi-selection is *of*. This was hardcoded to "Tracks", so selecting albums reported
/// "3 Tracks Selected" — a live user read it as the selection having quietly turned into something
/// else (`docs/12-decisions.md`). A mixed selection is neither, so it says "Items".
fn selection_noun(selected: &[MediaItem]) -> &'static str {
    let first = match selected.first() {
        Some(item) => item,
        None => return "Items",
    };
    let noun = match first {
        MediaItem::Track(_) => "Tracks",
        MediaItem::Album(_) => "Albums",
        MediaItem::Artist(_) => "Artists",
        MediaItem::Playlist(_) => "Playlists",
        MediaItem::Folder(_) => "Folders",
        MediaItem::Genre(_) => "Genres",
        MediaItem::SectionHeader(_) => "Items",
    };
    let same_kind = selected
        .iter()
        .all(|item| std::mem::discriminant(item) == std::mem::discriminant(first));
    if same_kind { noun } else { "Items" }
}

fn render_artist_body(f: &mut Frame, area: Rect, artist: &Artist, theme: &Theme) {
    let fg = style::style(theme, Role::Fg);
    let mut header = vec![Line::from(Span::styled(artist.name.clone(), fg))];
    // Omitted rather than shown as `0 albums, 0 tracks` when unknown — see `Artist::counts_summary`.
    if let Some(counts) = artist.counts_summary() {
        header.push(Line::from(Span::styled(counts, fg)));
    }
    header.push(Line::from(Span::styled(artist.genres.join(", "), fg)));
    draw_rows(f, area, &header);

    let overview_area = Rect::new(
        area.x,
        area.y + header.len() as u16,
        area.width,
        area.height.saturating_sub(header.len() as u16),
    );
    if let Some(overview) = &artist.overview {
        render_overview(f, overview_area, overview, theme);
    }
}

fn render_album_body(f: &mut Frame, area: Rect, album: &Album, theme: &Theme) {
    let fg = style::style(theme, Role::Fg);
    let favourite = if album.is_favorite {
        "♡ Favourite"
    } else {
        ""
    };
    let lines = vec![
        Line::from(Span::styled(album.name.clone(), fg)),
        Line::from(Span::styled(album.album_artist_names.join(", "), fg)),
        Line::from(Span::styled(
            album.year.map(|y| y.to_string()).unwrap_or_default(),
            fg,
        )),
        Line::from(Span::styled(
            format!(
                "{} tracks, {}",
                album.track_count,
                format_duration(album.total_duration)
            ),
            fg,
        )),
        Line::from(Span::styled(favourite.to_string(), fg)),
    ];
    draw_rows(f, area, &lines);
}

/// `09-04`: "the inspector displays which path was taken" — `ReplayGain (album): −6.2 dB`,
/// `Emby normalization: −4.1 dB`, or `no gain`, per `player.applied_gain`
/// (`loxia_audio::replaygain::resolve_gain`'s own return value for the current track).
fn replay_gain_source_label(applied: AppliedGain, db: Option<f32>) -> String {
    match applied {
        AppliedGain::Tags(mode) => {
            let mode_label = match mode {
                ReplayGainMode::Album => "album",
                ReplayGainMode::Track => "track",
                // Unreachable in practice — `resolve_gain` never returns `Tags(Off)` — but a
                // display label must still be total, not a panic, if that ever changes.
                ReplayGainMode::Off => "off",
            };
            match db {
                Some(db) => format!("ReplayGain ({mode_label}): {db:+.1} dB"),
                None => format!("ReplayGain ({mode_label})"),
            }
        }
        AppliedGain::Normalization(db) => format!("Emby normalization: {db:+.1} dB"),
        AppliedGain::None => "no gain".to_string(),
    }
}

fn render_track_body(f: &mut Frame, area: Rect, track: &Track, state: &AppState, theme: &Theme) {
    let fg = style::style(theme, Role::Fg);
    let favourite = if track.is_favorite {
        "♡ Favourite"
    } else {
        ""
    };
    let disc_track = format!(
        "Disc {}, Track {}",
        track.disc_number.unwrap_or(1),
        track.track_number.unwrap_or(0)
    );
    let bitrate = track
        .format
        .bitrate_bps
        .map(|b| format!("{} kbps", b / 1000))
        .unwrap_or_default();

    // "Applied" ReplayGain only means something once the audio engine has actually applied one
    // to *this* track — i.e. it's the currently playing entry. Otherwise fall back to the raw
    // tag, or omit entirely if neither is known.
    let now_playing = state
        .player
        .current
        .and_then(|eid| state.queue.entries.iter().find(|e| e.entry_id == eid))
        .map(|e| &e.track.id)
        == Some(&track.id);
    let replay_gain = if now_playing {
        replay_gain_source_label(state.player.applied_gain, state.player.applied_gain_db)
    } else {
        track
            .replay_gain
            .and_then(|rg| rg.track_gain_db)
            .map(|db| format!("ReplayGain (tag): {db:+.1} dB"))
            .unwrap_or_default()
    };

    let lines = vec![
        Line::from(Span::styled(track.name.clone(), fg)),
        Line::from(Span::styled(track.artist_names.join(", "), fg)),
        Line::from(Span::styled(track.album_name.clone(), fg)),
        Line::from(Span::styled(
            format!(
                "{} · {}",
                track.year.map(|y| y.to_string()).unwrap_or_default(),
                disc_track
            ),
            fg,
        )),
        Line::from(Span::styled(format_duration(track.duration), fg)),
        Line::from(Span::styled(track.format.summary(), fg)),
        Line::from(Span::styled(bitrate, fg)),
        Line::from(Span::styled(
            format!("Played {} times", track.play_count),
            fg,
        )),
        Line::from(Span::styled(replay_gain, fg)),
        Line::from(Span::styled(favourite.to_string(), fg)),
        // `docs/07-ui-spec.md` §6 also asks for "availability" here — no data source exists yet
        // (cache/offline state is phase 08's job); omitted until then, see `docs/12-decisions.md`.
    ];
    draw_rows(f, area, &lines);
}

fn render_generic_body(f: &mut Frame, area: Rect, item: &MediaItem, theme: &Theme) {
    let fg = style::style(theme, Role::Fg);
    let lines = vec![Line::from(Span::styled(
        item.display_name().to_string(),
        fg,
    ))];
    draw_rows(f, area, &lines);
}

/// Wraps `overview` to `area`'s width, clips to its height, and appends a `▾` indicator on the
/// last visible line when there is more text than fits.
fn render_overview(f: &mut Frame, area: Rect, overview: &str, theme: &Theme) {
    if area.height == 0 || overview.is_empty() {
        return;
    }
    let wrapped = text::wrap(overview, area.width as usize);
    let visible_rows = area.height as usize;
    let overflowing = wrapped.len() > visible_rows;
    let fg = style::fg(theme, Role::Fg);

    for (i, line) in wrapped.iter().take(visible_rows).enumerate() {
        let is_last_visible = overflowing && i == visible_rows - 1;
        let text = if is_last_visible {
            format!(
                "{} ▾",
                text::truncate(line, area.width.saturating_sub(2) as usize)
            )
        } else {
            line.clone()
        };
        let row = Rect::new(area.x, area.y + i as u16, area.width, 1);
        f.render_widget(Paragraph::new(Line::from(Span::styled(text, fg))), row);
    }
}

fn action_label(id: ActionId) -> &'static str {
    match id {
        ActionId::QueueArtistOnly => "Play (Replace Queue)",
        ActionId::QueueFullContext => "Add to Queue",
        ActionId::InsertNext => "Insert Next",
        ActionId::InstantMix => "Instant Mix",
        ActionId::AddToPlaylist => "Add to Playlist",
        ActionId::ToggleDownload => "Download",
        ActionId::ToggleFavorite => "Favourite",
        ActionId::DeletePlaylist => "Delete Playlist",
        _ => "",
    }
}

/// Which contextual actions apply to the current selection — never a hardcoded key, only the
/// `ActionId`; `render_actions` resolves each one's displayed key through `keymap.hint_for`, so a
/// remap always shows correctly here.
fn actions_for(
    item: Option<&MediaItem>,
    selected_count: usize,
    selection_is_tracks: bool,
    tab: Tab,
) -> Vec<ActionId> {
    let mut actions = Vec::new();
    // Track-only actions. `selected_count > 1` used to be enough on its own, which offered "Add to
    // Playlist" and "Download" over a selection of albums — neither of which does anything with a
    // non-track row (`modals::save_playlist` filters the selection down to `Track`s).
    let is_track = matches!(item, Some(MediaItem::Track(_))) || selection_is_tracks;
    let queueable = selected_count > 1
        || matches!(
            item,
            Some(
                MediaItem::Artist(_)
                    | MediaItem::Album(_)
                    | MediaItem::Track(_)
                    | MediaItem::Playlist(_)
                    | MediaItem::Genre(_)
                    | MediaItem::Folder(_)
            )
        );

    if queueable {
        actions.push(ActionId::QueueArtistOnly);
        // Listed alongside it, not left to be discovered: on an `Appears On` album these two are a
        // genuine either/or — the selected artist's tracks, or the whole compilation — and until
        // now only the first was ever shown, so the second was invisible (`docs/12-decisions.md`).
        actions.push(ActionId::QueueFullContext);
        actions.push(ActionId::InsertNext);
        actions.push(ActionId::InstantMix);
    }
    if is_track {
        actions.push(ActionId::AddToPlaylist);
    }
    // Downloading is not track-only: `loxia_cache::downloads` expands an album, artist or playlist
    // itself, and a user pressing `d` on an album row is exactly the case that wants it.
    if selection_is_tracks
        || matches!(
            item,
            Some(
                MediaItem::Track(_)
                    | MediaItem::Album(_)
                    | MediaItem::Artist(_)
                    | MediaItem::Playlist(_)
            )
        )
    {
        actions.push(ActionId::ToggleDownload);
    }
    if matches!(
        item,
        Some(MediaItem::Artist(_) | MediaItem::Album(_) | MediaItem::Track(_))
    ) {
        actions.push(ActionId::ToggleFavorite);
    }
    if tab == Tab::Playlists && matches!(item, Some(MediaItem::Playlist(_))) {
        actions.push(ActionId::DeletePlaylist);
    }
    actions
}

/// The action's own label, specialised for what pressing it would actually do to *this* item.
///
/// The two queue actions are generic everywhere else ("Play (Replace Queue)" / "Add to Queue"), but
/// on an album the artist merely *appears on* they mean two very different things, and the
/// difference is the whole reason both exist. Naming the artist makes the choice legible without
/// the user having to know the rule.
fn action_label_for(action: ActionId, item: Option<&MediaItem>, state: &AppState) -> String {
    let Some(MediaItem::Album(album)) = item else {
        return action_label(action).to_string();
    };
    let AlbumRelation::AppearsOn { context_artist } = &album.relation else {
        return action_label(action).to_string();
    };
    match action {
        ActionId::QueueArtistOnly => match context_artist_name(state, context_artist) {
            Some(name) => format!("Play {name}'s tracks only"),
            None => "Play this artist's tracks only".to_string(),
        },
        ActionId::QueueFullContext => "Add whole album".to_string(),
        _ => action_label(action).to_string(),
    }
}

/// The name behind `AlbumRelation::AppearsOn`'s context artist id. The relation carries only the
/// id, so this looks the artist up in the Miller stack the user drilled through to get here —
/// where it is always present, that column being what produced this album in the first place.
fn context_artist_name(state: &AppState, artist: &loxia_core::model::ItemId) -> Option<String> {
    state
        .nav
        .per_tab_stacks
        .get(&state.nav.active_tab)?
        .iter()
        .flat_map(|col| col.items.iter())
        .find_map(|item| match item {
            MediaItem::Artist(a) if &a.id == artist => Some(a.name.clone()),
            _ => None,
        })
}

fn render_actions(
    f: &mut Frame,
    area: Rect,
    actions: &[ActionId],
    item: Option<&MediaItem>,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
) {
    if actions.is_empty() || area.height == 0 {
        return;
    }
    let header_row = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Actions:",
            style::fg(theme, Role::Dim),
        ))),
        header_row,
    );

    for (i, &action) in actions.iter().enumerate() {
        let y = area.y + 1 + i as u16;
        if y >= area.y + area.height {
            break;
        }
        let row = Rect::new(area.x, y, area.width, 1);
        let hint = state.keymap.hint_for(action);
        let text = format!("[{hint}] {}", action_label_for(action, item, state));
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                style::style(theme, Role::Fg),
            ))),
            row,
        );
        hits.push(row, HitTarget::InspectorAction(action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::keymap::KeyMap;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16, state: &AppState) -> (String, HitMap) {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                let mut off = crate::widgets::album_art::ArtOff::default();
                render(f, area, state, &theme, &mut hits, &mut off.art());
            })
            .unwrap();
        (format!("{:?}", terminal.backend().buffer()), hits)
    }

    fn state_with_keymap() -> AppState {
        AppState {
            keymap: KeyMap::defaults(),
            ..AppState::default()
        }
    }

    fn artist_state() -> AppState {
        let mut state = state_with_keymap();
        let a = fixtures::artist("Boy Harsher");
        let mut col = loxia_core::state::nav::Column::new(
            loxia_core::state::nav::ColumnKind::Artists,
            "Artists",
        );
        col.items = vec![MediaItem::Artist(Artist {
            overview: Some("A synth-heavy darkwave duo from Massachusetts.".to_string()),
            ..a
        })];
        state.nav.active_tab = Tab::Artists;
        state.nav.per_tab_stacks.insert(Tab::Artists, vec![col]);
        state.nav.focus = loxia_core::state::nav::NavFocus::Column(0);
        state
    }

    /// `ui.show_inspector_art = false` must give the rows back to the metadata *and* issue no
    /// fetch — the switch exists to spend nothing on artwork, not merely to hide it.
    #[test]
    fn turning_off_inspector_art_reclaims_its_rows_and_fetches_nothing() {
        let mut state = artist_state();
        state.config.ui.show_inspector_art = true;
        let (with_art, _) = render_at(40, 20, &state);

        state.config.ui.show_inspector_art = false;
        let (without_art, _) = render_at(40, 20, &state);

        let row_of = |s: &str, needle: &str| s.lines().position(|l| l.contains(needle));
        assert!(
            row_of(&without_art, "Boy Harsher") < row_of(&with_art, "Boy Harsher"),
            "the metadata should move up into the reclaimed rows:\n{without_art}"
        );

        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        let mut effects = Vec::new();
        terminal
            .draw(|f| {
                let area = f.area();
                let mut off = crate::widgets::album_art::ArtOff::default();
                effects = render(f, area, &state, &theme, &mut hits, &mut off.art());
            })
            .unwrap();
        assert!(
            effects.is_empty(),
            "no artwork should be requested when the setting is off: {effects:?}"
        );
    }

    #[test]
    fn inspector_snapshot_artist() {
        let state = artist_state();
        let (rendered, _) = render_at(40, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    /// On an album the artist merely appears on, `Enter` queues only that artist's tracks and
    /// `Shift+Enter` the whole compilation. Both are real choices, and until now the inspector
    /// listed only the first, under a label that said nothing about the filtering
    /// (`docs/12-decisions.md`). Both must be listed, and named for what they do here.
    #[test]
    fn appears_on_album_spells_out_both_queueing_choices() {
        let mut state = state_with_keymap();
        let artist = fixtures::artist("Boy Harsher");
        let mut artists_col = loxia_core::state::nav::Column::new(
            loxia_core::state::nav::ColumnKind::Artists,
            "Artists",
        );
        artists_col.items = vec![MediaItem::Artist(artist.clone())];

        let mut compilation = fixtures::album("Various Artists Vol. 1", 2020, &artist);
        compilation.relation = AlbumRelation::AppearsOn {
            context_artist: artist.id.clone(),
        };
        let mut albums_col = loxia_core::state::nav::Column::new(
            loxia_core::state::nav::ColumnKind::Albums {
                of_artist: Some(artist.id.clone()),
            },
            "Albums",
        );
        albums_col.items = vec![MediaItem::Album(compilation)];

        state.nav.active_tab = Tab::Artists;
        state
            .nav
            .per_tab_stacks
            .insert(Tab::Artists, vec![artists_col, albums_col]);
        state.nav.focus = loxia_core::state::nav::NavFocus::Column(1);

        let (rendered, _) = render_at(50, 24, &state);

        assert!(
            rendered.contains("Play Boy Harsher's tracks only"),
            "the artist-filtered choice should name the artist:\n{rendered}"
        );
        assert!(
            rendered.contains("Add whole album"),
            "the full-album choice must be offered too:\n{rendered}"
        );
    }

    /// Away from an `Appears On` row the two keep their generic meanings — the specialised wording
    /// must not leak onto an ordinary album, where there is no filtering to explain.
    #[test]
    fn a_primary_album_keeps_the_generic_queue_labels() {
        let mut state = fixtures::fixture_miller_3col();
        state.keymap = KeyMap::defaults();
        state.nav.focus = loxia_core::state::nav::NavFocus::Column(1);

        let (rendered, _) = render_at(50, 24, &state);

        assert!(rendered.contains("Play (Replace Queue)"));
        assert!(rendered.contains("Add to Queue"));
        assert!(!rendered.contains("tracks only"));
    }

    #[test]
    fn inspector_snapshot_album() {
        let state = fixtures::fixture_miller_3col();
        let mut state = state;
        state.nav.focus = loxia_core::state::nav::NavFocus::Column(1);
        let (rendered, _) = render_at(40, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn inspector_snapshot_track() {
        let mut state = fixtures::fixture_miller_3col();
        state.keymap = KeyMap::defaults();
        let (rendered, _) = render_at(40, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn inspector_snapshot_multiselect() {
        let mut state = fixtures::fixture_visual_select();
        state.keymap = KeyMap::defaults();
        let (rendered, _) = render_at(40, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn inspector_snapshot_empty() {
        let state = fixtures::fixture_empty();
        let (rendered, _) = render_at(40, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn multiselect_total_duration_is_summed() {
        let mut state = fixtures::fixture_visual_select();
        state.keymap = KeyMap::defaults();
        let (rendered, _) = render_at(40, 20, &state);
        // fixture_visual_select selects 3 of 5 tracks, each 180s (3:00) long -> 9:00 total.
        assert!(rendered.contains("9:00"));
    }

    #[test]
    fn action_hints_come_from_keymap() {
        let mut state = fixtures::fixture_miller_3col();
        state.keymap = KeyMap::defaults();
        let before = render_at(40, 20, &state).0;
        assert!(before.contains("[ctrl+p] Add to Playlist"));

        // `shift+p` normalises to the plain uppercase `Char('P')` chord (`03-04`'s own rule: a
        // shifted lowercase letter *is* its uppercase form) — the remapped hint renders as
        // `ctrl+P`, not `ctrl+shift+p`.
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("add_to_playlist".to_string(), "ctrl+shift+p".to_string());
        let (keymap, _warnings) = KeyMap::from_config(&overrides);
        state.keymap = keymap;
        let after = render_at(40, 20, &state).0;
        assert!(after.contains("[ctrl+P] Add to Playlist"));
        assert!(!after.contains("[ctrl+p] Add to Playlist"));
    }

    /// The multi-select panel hardcoded "Tracks", so selecting albums reported "3 Tracks
    /// Selected" — reported from live use as the legend claiming the wrong thing.
    #[test]
    fn the_multiselect_noun_follows_what_is_selected() {
        let a = fixtures::artist("A");
        let alb1 = fixtures::album("One", 2020, &a);
        let alb2 = fixtures::album("Two", 2021, &a);
        let track = fixtures::track("T", 1, &alb1, &[&a]);

        assert_eq!(
            selection_noun(&[
                MediaItem::Album(alb1.clone()),
                MediaItem::Album(alb2.clone())
            ]),
            "Albums"
        );
        assert_eq!(
            selection_noun(&[
                MediaItem::Track(track.clone()),
                MediaItem::Track(track.clone())
            ]),
            "Tracks"
        );
        assert_eq!(
            selection_noun(&[MediaItem::Artist(a.clone()), MediaItem::Artist(a.clone())]),
            "Artists"
        );
        // Mixed kinds are neither.
        assert_eq!(
            selection_noun(&[MediaItem::Album(alb1), MediaItem::Track(track)]),
            "Items"
        );
    }

    #[test]
    fn action_list_varies_by_selection_type() {
        let artist = actions_for(
            Some(&MediaItem::Artist(fixtures::artist("A"))),
            0,
            false,
            Tab::Artists,
        );
        assert!(artist.contains(&ActionId::ToggleFavorite));
        assert!(!artist.contains(&ActionId::AddToPlaylist));

        let a = fixtures::artist("A");
        let alb = fixtures::album("Alb", 2020, &a);
        let track = fixtures::track("T", 1, &alb, &[&a]);
        let track_actions = actions_for(Some(&MediaItem::Track(track)), 0, false, Tab::Artists);
        assert!(track_actions.contains(&ActionId::AddToPlaylist));
        assert!(track_actions.contains(&ActionId::ToggleDownload));

        let multi_tracks = actions_for(None, 3, true, Tab::Artists);
        assert!(multi_tracks.contains(&ActionId::AddToPlaylist));

        // Selecting albums is not selecting tracks: the track-only actions must not be offered,
        // since neither does anything with a non-track row.
        let multi_albums = actions_for(None, 3, false, Tab::Albums);
        assert!(multi_albums.contains(&ActionId::QueueArtistOnly));
        assert!(multi_albums.contains(&ActionId::InsertNext));
        assert!(!multi_albums.contains(&ActionId::AddToPlaylist));

        let playlist = loxia_core::model::Playlist {
            id: loxia_core::model::ItemId::from("p1"),
            name: "My Playlist".to_string(),
            overview: None,
            track_count: 0,
            total_duration: Duration::ZERO,
            can_edit: true,
            is_favorite: false,
        };
        let playlist_actions = actions_for(
            Some(&MediaItem::Playlist(playlist)),
            0,
            false,
            Tab::Playlists,
        );
        assert!(playlist_actions.contains(&ActionId::DeletePlaylist));
    }

    #[test]
    fn actions_register_hit_targets() {
        let mut state = fixtures::fixture_miller_3col();
        state.keymap = KeyMap::defaults();
        let (_, hits) = render_at(40, 20, &state);
        assert!(
            (0..20).any(|y| matches!(hits.hit(1, y), Some(HitTarget::InspectorAction(_)))),
            "expected at least one InspectorAction hit target"
        );
    }

    #[test]
    fn overview_wraps_and_clips() {
        let mut state = artist_state();
        if let Some(col) = state
            .nav
            .per_tab_stacks
            .get_mut(&Tab::Artists)
            .and_then(|s| s.first_mut())
            && let MediaItem::Artist(a) = &mut col.items[0]
        {
            a.overview = Some("word ".repeat(200));
        }
        // Tall enough that the art placeholder, header lines, and action footer leave some room
        // for the overview, but not nearly enough for 200 words of it.
        let (rendered, _) = render_at(30, 30, &state);
        assert!(rendered.contains('▾'));
    }

    #[test]
    fn scroll_indicator_only_when_overflowing() {
        let state = artist_state();
        let (rendered, _) = render_at(60, 30, &state);
        assert!(!rendered.contains('▾'));
    }

    #[test]
    fn narrow_inspector_snapshot_24_cells() {
        let mut state = fixtures::fixture_miller_3col();
        state.keymap = KeyMap::defaults();
        let (rendered, _) = render_at(24, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn inspector_shows_gain_source() {
        insta::assert_snapshot!(
            "gain_source_tags_album",
            replay_gain_source_label(AppliedGain::Tags(ReplayGainMode::Album), Some(-6.2))
        );
        insta::assert_snapshot!(
            "gain_source_tags_track",
            replay_gain_source_label(AppliedGain::Tags(ReplayGainMode::Track), Some(-3.4))
        );
        insta::assert_snapshot!(
            "gain_source_normalization",
            replay_gain_source_label(AppliedGain::Normalization(-4.1), Some(-4.1))
        );
        insta::assert_snapshot!(
            "gain_source_none",
            replay_gain_source_label(AppliedGain::None, None)
        );
    }
}
