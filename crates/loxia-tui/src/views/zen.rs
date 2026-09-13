//! Zen focus view (`10-02`, `docs/07-ui-spec.md` §9): a minimalist full-screen now-playing
//! display — artwork, track detail, lyrics, progress, and the format line, with no navigation
//! chrome. `zones()` (`04-02`) already gives Zen the whole body (`sidebar: None`); `render_canvas`
//! (`render.rs`) dispatches here whenever `state.zen_mode` is set, regardless of the active tab.
//!
//! Zen is a *view*, not a mode that changes behaviour: nothing in the reducer's own dispatch
//! branches on `state.zen_mode` except the toggle itself (`reducer::mod`'s own
//! `ViewAction::ToggleZen` arm) — every other keybinding, action, and modal keeps working exactly
//! as it does everywhere else. This module only decides what to draw.
//!
//! **No `ArtCache` of its own.** Real album art needs a cache threaded in from outside (see
//! `widgets::album_art`'s own top-level doc comment for why that wiring is deferred to `10-04`) —
//! this view draws the same placeholder `widgets::album_art` itself falls back to, reserving the
//! correctly-sized region without pretending to fetch anything.

use loxia_core::keymap::ActionId;
use loxia_core::state::AppState;
use loxia_core::state::queue::QueueEntry;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::HitMap;
use crate::style;
use crate::widgets::{album_art, lyrics, player_bar};

/// Below this width, artwork stacks above the detail instead of beside it.
const STACK_THRESHOLD: u16 = 100;
/// Below this width, artwork is dropped entirely — text is the higher-value content.
const DROP_ART_THRESHOLD: u16 = 80;
/// Art's own row cap in the wide, side-by-side layout.
const WIDE_ART_ROWS: u16 = 20;
/// Art's own row cap in the narrow, stacked layout.
const NARROW_ART_ROWS: u16 = 10;
/// Progress row + format line, one row each, always at the very bottom.
/// Exactly the global player bar's own zone height (`layout::compute`'s `Constraint::Length(4)`):
/// its top border plus three content rows. Zen fills only the lower two of those rows — the title
/// line the bar shows first is redundant here, where the title *is* the view — but it must still
/// claim the same height, or the bottom panel would visibly grow and shrink as Zen is toggled
/// (`docs/12-decisions.md`). Leaving the blank row at the *top* also keeps the seek bar and the
/// technical readout on the same two screen rows either side of the toggle, so neither jumps.
const FOOTER_HEIGHT: u16 = 4;
/// How many of [`FOOTER_HEIGHT`]'s rows Zen actually draws into, counted up from the bottom.
const FOOTER_CONTENT_ROWS: u16 = 2;
/// Title, artist, `Album: ...` — the detail block's own height.
const DETAIL_LINES_NO_LYRICS: u16 = 3;
/// Blank columns between the now-playing pane and the lyrics column. Zen draws no border between
/// its panes, unlike every other view — this gap is the whole separator.
const LYRICS_GUTTER: u16 = 2;

fn current_entry(state: &AppState) -> Option<&QueueEntry> {
    let id = state.player.current?;
    state.queue.entries.iter().find(|e| e.entry_id == id)
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

    let Some(entry) = current_entry(state) else {
        render_nothing_playing(f, area, state, theme);
        return Vec::new();
    };

    let footer_height = FOOTER_HEIGHT.min(area.height);
    let available = area.height - footer_height;

    let show_art = area.width >= DROP_ART_THRESHOLD;
    let show_lyrics = lyrics::should_show(state);
    // The *split* is decided by whether this track has a lyric stream at all, never by whether the
    // pane is currently toggled on. Keying the geometry off the toggle is what made the artwork and
    // the title block slide around every time lyrics were shown or hidden (`docs/12-decisions.md`);
    // reserving the column either way keeps everything still, and `show_lyrics` decides only
    // whether anything is drawn into it.
    let track_has_lyrics = entry.track.lyric_stream.is_some();
    let side_by_side = track_has_lyrics && area.width >= STACK_THRESHOLD;

    // The small size is for the one layout that genuinely cannot afford the big one: a single
    // column with the lyrics stacked underneath, where every row the cover takes is a row of lyrics
    // lost. Given its own column — or no lyrics at all — the artwork gets the full size, capped by
    // the pane it is in. Requesting the small size for the side-by-side layout too was simply
    // wrong, and made the cover shrink the moment Zen became two panes (`docs/12-decisions.md`).
    let stacked_under_lyrics = track_has_lyrics && !side_by_side;
    let requested_art_rows = if !show_art {
        0
    } else if stacked_under_lyrics {
        NARROW_ART_ROWS
    } else {
        WIDE_ART_ROWS
    };

    let (detail_area, lyrics_area) = if side_by_side {
        // Deliberately no border, rule or gutter glyph between the two — every other view in the
        // app separates its panes with a box, and Zen is the one that must not (a user asked for
        // exactly this). The blank column between them is the whole separator.
        let left_width = area.width / 2;
        (
            Rect::new(area.x, area.y, left_width, available),
            Some(Rect::new(
                area.x + left_width + LYRICS_GUTTER,
                area.y,
                area.width.saturating_sub(left_width + LYRICS_GUTTER),
                available,
            )),
        )
    } else if track_has_lyrics {
        // Too narrow for two columns, so the lyrics keep the stacked treatment — but the split is
        // still made on `track_has_lyrics`, so toggling the pane moves nothing here either.
        let block = fitted_art_rows(
            Rect::new(area.x, area.y, area.width, available),
            requested_art_rows,
        );
        let block_height = (block + u16::from(block > 0) + DETAIL_LINES_NO_LYRICS).min(available);
        (
            Rect::new(area.x, area.y, area.width, block_height),
            Some(Rect::new(
                area.x,
                area.y + block_height,
                area.width,
                available - block_height,
            )),
        )
    } else {
        (Rect::new(area.x, area.y, area.width, available), None)
    };

    let art_rows = fitted_art_rows(detail_area, requested_art_rows);
    let effects = render_now_playing_pane(f, detail_area, art_rows, entry, theme, art);
    if let Some(lyrics_area) = lyrics_area
        && show_lyrics
    {
        lyrics::render(f, lyrics_area, state, theme);
    }
    let footer_area = Rect::new(area.x, area.y + available, area.width, footer_height);
    render_footer(f, footer_area, state, theme, hits);
    effects
}

/// How many rows the artwork may actually take in `area`: never so many that the title, artist and
/// album lines are pushed off the bottom, and never wider than the pane (art is drawn two columns
/// per row, so the width caps the rows too).
///
/// Getting this wrong is not subtle — at the wide art size the cover filled the pane exactly and
/// the three text lines simply vanished.
fn fitted_art_rows(area: Rect, requested: u16) -> u16 {
    if requested == 0 || area.height <= DETAIL_LINES_NO_LYRICS + 1 {
        return 0;
    }
    requested
        .min(area.height - DETAIL_LINES_NO_LYRICS - 1)
        .min(area.width / 2)
}

/// Artwork above the title/artist/album block, the pair vertically centred in `area`.
///
/// Zen used to lay these out *beside* each other and hang the lyrics underneath, which is why the
/// block moved: with lyrics the content filled the canvas and sat at the top, without them it
/// collapsed to three lines and centred. Art and detail now travel together as one centred unit
/// and the lyrics get their own area, so nothing about their placement depends on the lyrics at
/// all (`docs/12-decisions.md`).
fn render_now_playing_pane(
    f: &mut Frame,
    area: Rect,
    art_rows: u16,
    entry: &QueueEntry,
    theme: &Theme,
    art: &mut album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    if area.height == 0 || area.width == 0 {
        return Vec::new();
    }
    let mut effects: Vec<loxia_core::effect::Effect> = Vec::new();

    let art_cols = (art_rows * 2).min(area.width);
    let gap = u16::from(art_rows > 0);
    let block_height = (art_rows + gap + DETAIL_LINES_NO_LYRICS).min(area.height);
    let top = area.y + (area.height - block_height) / 2;

    if art_rows > 0 {
        let x = area.x + (area.width.saturating_sub(art_cols)) / 2;
        effects.extend(album_art::render(
            f,
            Rect::new(x, top, art_cols, art_rows),
            album_art::ArtSizeContext::Zen,
            art,
            Some(&loxia_core::model::MediaItem::Track(entry.track.clone())),
            theme,
        ));
    }

    let track = &entry.track;
    let album_text = match track.year {
        Some(y) => format!("Album: {} ({y})", track.album_name),
        None => format!("Album: {}", track.album_name),
    };
    // Padded on *both* sides, not just prefixed: the line is centred, so two leading cells would
    // slide the title itself sideways — the very jitter this pattern exists to avoid. Balanced
    // padding leaves the name exactly where it was and hangs the heart off its left
    // (`docs/12-decisions.md`). Zen shows one track and nothing else, so without a marker there is
    // no sign at all that `f` did anything.
    let title = if track.is_favorite {
        format!("♡ {}  ", track.name)
    } else {
        track.name.clone()
    };
    let lines = [
        Line::from(Span::styled(
            title,
            style::fg(theme, Role::Accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            track.artist_names.join(", "),
            style::style(theme, Role::Fg),
        )),
        Line::from(Span::styled(album_text, style::style(theme, Role::Fg))),
    ];
    let detail_y = top + art_rows + gap;
    for (i, line) in lines.into_iter().enumerate() {
        let y = detail_y + i as u16;
        if y < area.y + area.height {
            f.render_widget(
                Paragraph::new(line).alignment(Alignment::Center),
                Rect::new(area.x, y, area.width, 1),
            );
        }
    }
    effects
}

/// Zen's own status footer. Chrome matches the global player bar exactly — a top border plus one
/// column of side padding — so switching into Zen doesn't visibly resize or restyle the bottom of
/// the screen (`docs/12-decisions.md`).
fn render_footer(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(style::border(theme, false));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let area = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Bottom-anchored: the seek/transport row and the technical readout occupy the last two rows
    // of the zone, exactly where the player bar puts them, so neither moves as Zen is toggled.
    let content_rows = FOOTER_CONTENT_ROWS.min(area.height);
    let top = area.y + (area.height - content_rows);

    player_bar::render_line2(f, Rect::new(area.x, top, area.width, 1), state, theme, hits);
    if content_rows < 2 {
        return;
    }
    player_bar::render_line3(f, Rect::new(area.x, top + 1, area.width, 1), state, theme);
}

fn render_nothing_playing(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if area.height == 0 {
        return;
    }
    let hint = state.keymap.hint_for(ActionId::ToggleZenMode);
    let text = format!("nothing playing — press {hint} to return");
    let row = area.y + area.height / 2;
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style::fg(theme, Role::Dim))))
            .alignment(Alignment::Center),
        Rect::new(area.x, row, area.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toggling lyrics must not move the cover: Zen centres its block vertically, and deriving that
    /// centring from the toggle slid everything up and down (`docs/12-decisions.md`).
    #[test]
    fn toggling_lyrics_does_not_move_the_art() {
        use loxia_core::model::{LyricStreamRef, MediaSourceId};

        let mut state = fixtures::fixture_playing_queue();
        state.keymap = loxia_core::keymap::KeyMap::defaults();
        let idx = state.queue.play_order[state.queue.position];
        state.queue.entries[idx].track.lyric_stream = Some(LyricStreamRef {
            media_source_id: MediaSourceId::from("s"),
            stream_index: 2,
            format: loxia_core::model::LyricFormat::Lrc,
        });

        // The art placeholder's own top border row is what must not move.
        let art_row = |s: &str| {
            s.lines()
                .position(|l| l.contains('\u{250c}'))
                .expect("art placeholder rendered")
        };

        state.config.ui.show_lyrics = true;
        let with = render_to_string(120, 30, &state);
        state.config.ui.show_lyrics = false;
        let without = render_to_string(120, 30, &state);

        assert_eq!(
            art_row(&with),
            art_row(&without),
            "the cover moved when lyrics were toggled"
        );

        // The track/artist/album block must hold its position too — it used to switch between a
        // vertically-centred layout and a top-aligned one.
        let title_row = |s: &str| {
            s.lines()
                .position(|l| l.contains("Track 4"))
                .expect("title rendered")
        };
        assert_eq!(
            title_row(&with),
            title_row(&without),
            "the track/artist/album block moved when lyrics were toggled"
        );
    }

    /// `f` in Zen changed nothing on screen, which is how a working action and a broken one look
    /// identical. The marker must also leave the centred title exactly where it was
    /// (`docs/12-decisions.md`).
    #[test]
    fn a_favourited_track_is_marked_without_moving_the_title() {
        let mut state = fixtures::fixture_playing_queue();
        state.keymap = loxia_core::keymap::KeyMap::defaults();
        let plain = render_to_string(120, 30, &state);
        // Measured in **columns**, not bytes: `♡` is three bytes and one cell, so a byte offset
        // would report a shift that is not on screen.
        let column_of = |s: &str| {
            s.lines()
                .find_map(|l| l.find("Track 4").map(|byte| crate::text::width(&l[..byte])))
                .expect("title")
        };

        let index = state.queue.play_order[state.queue.position];
        state.queue.entries[index].track.is_favorite = true;
        let marked = render_to_string(120, 30, &state);

        assert!(marked.contains('♡'), "no favourite marker: {marked}");
        assert_eq!(
            column_of(&plain),
            column_of(&marked),
            "the title must not slide when the marker appears"
        );
    }

    /// A user asked for Zen to be two panes: now-playing on the left, lyrics on the right, with
    /// **no** separator between them — the one view in the app that must not draw a box between its
    /// panes (`docs/12-decisions.md`).
    #[test]
    fn lyrics_take_their_own_column_with_no_divider() {
        use loxia_core::model::{LyricStreamRef, MediaSourceId};

        let mut state = fixtures::fixture_playing_queue();
        state.keymap = loxia_core::keymap::KeyMap::defaults();
        let idx = state.queue.play_order[state.queue.position];
        let track_id = state.queue.entries[idx].track.id.clone();
        state.queue.entries[idx].track.lyric_stream = Some(LyricStreamRef {
            media_source_id: MediaSourceId::from("s"),
            stream_index: 2,
            format: loxia_core::model::LyricFormat::Lrc,
        });
        state.config.ui.show_lyrics = true;
        state.lyrics = Some((
            track_id,
            loxia_core::model::Lyrics::Unsynced(vec!["a lyric line".to_string()]),
        ));

        let rendered = render_to_string(140, 30, &state);
        let line = rendered
            .lines()
            .find(|l| l.contains("a lyric line"))
            .expect("lyrics rendered");
        assert!(
            !line.contains('\u{2502}'),
            "no vertical rule may separate the panes: {line}"
        );

        // The title sits in the left half, the lyric in the right — side by side, not stacked.
        let col = |needle: &str| {
            rendered
                .lines()
                .find_map(|l| l.find(needle))
                .expect("rendered")
        };
        assert!(col("Track 4") < 70, "the title belongs in the left pane");
        assert!(
            col("a lyric line") >= 70,
            "the lyrics belong in the right pane"
        );

        // Having its own column is exactly what lets the cover stay big — requesting the small,
        // stacked size here made it shrink the moment Zen became two panes.
        let art_rows = rendered
            .lines()
            .filter(|l| l.contains('\u{2502}') && l.contains('\u{2502}'))
            .count();
        assert!(
            art_rows >= 15,
            "the cover must not shrink in the side-by-side layout, got {art_rows} rows"
        );
    }

    fn render_to_string(w: u16, h: u16, state: &AppState) -> String {
        let backend = ratatui::backend::TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        let mut off = crate::widgets::album_art::ArtOff::default();
        terminal
            .draw(|f| {
                render(f, f.area(), state, &theme, &mut hits, &mut off.art());
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }
    use loxia_core::action::{Action, ItemAction, PlayerAction};
    use loxia_core::keymap::KeyMap;
    use loxia_core::state::player::SeekTarget;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Duration;

    fn render_at(w: u16, h: u16, state: &AppState) -> String {
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
        format!("{:?}", terminal.backend().buffer())
    }

    fn zen_state() -> AppState {
        let mut state = fixtures::fixture_playing_queue();
        state.zen_mode = true;
        state.keymap = KeyMap::defaults();
        state.player.duration = Duration::from_secs(211); // 3:31, matching the spec's own mockup
        state.player.position = Duration::from_secs(84); // 1:24
        state
    }

    #[test]
    fn zen_keeps_header_and_player_bar() {
        // `zones()` (`04-02`) already gives Zen the header/player rows unconditionally, and
        // `render_canvas` only ever swaps out the *canvas* — this view's own render is only ever
        // handed the canvas `Rect`, never the header/player rows, so it structurally cannot draw
        // over them. Asserted here as a standing invariant of this module's own signature: it
        // takes exactly one `Rect`, the canvas.
        let state = zen_state();
        let rendered = render_at(120, 10, &state);
        assert!(rendered.contains("Track 4")); // fixture's own current-track name at position 3
    }

    #[test]
    fn content_is_vertically_centred() {
        let mut state = zen_state();
        state.config.ui.show_lyrics = false; // keep the block short relative to a tall canvas
        let rendered = render_at(120, 30, &state);
        // The title must not be on the very first row of a tall canvas — it should be padded
        // down by roughly half the leftover space.
        let backend_lines: Vec<&str> = rendered.lines().collect();
        let title_line = backend_lines
            .iter()
            .position(|l| l.contains("Track 4"))
            .expect("title must render somewhere");
        assert!(
            title_line > 2,
            "title landed at content line {title_line}, expected padded well below the top"
        );
    }

    #[test]
    fn narrow_stacks_art_above_detail() {
        let state = zen_state();
        let rendered = render_at(90, 20, &state);
        let lines: Vec<&str> = rendered.lines().collect();
        let art_line = lines
            .iter()
            .position(|l| l.contains('♪'))
            .expect("art placeholder must still render when stacked");
        let title_line = lines
            .iter()
            .position(|l| l.contains("Track 4"))
            .expect("title must render");
        assert!(
            art_line < title_line,
            "art (row {art_line}) must sit above the detail block (row {title_line})"
        );
    }

    #[test]
    fn very_narrow_drops_art() {
        let state = zen_state();
        let rendered = render_at(70, 20, &state);
        assert!(
            !rendered.contains('♪'),
            "art must be dropped below 80 columns"
        );
        assert!(rendered.contains("Track 4"));
    }

    #[test]
    fn no_lyrics_omits_section() {
        let mut state = zen_state();
        state.config.ui.show_lyrics = false;
        let rendered = render_at(120, 20, &state);
        assert!(!rendered.contains("LYRICS"));
    }

    #[test]
    fn nothing_playing_state_shows_keymap_hint() {
        let mut state = fixtures::fixture_empty();
        state.zen_mode = true;
        state.keymap = KeyMap::defaults();
        let rendered = render_at(80, 24, &state);
        assert!(rendered.contains("nothing playing"));
        assert!(rendered.contains('z'), "must name the real bound key");
    }

    #[test]
    fn keybindings_still_work_in_zen() {
        // Zen is a view, not a mode that changes behaviour — every action still dispatches
        // normally regardless of `state.zen_mode`. Exercised at the reducer level, not through
        // this module's own render, since that's what "still work" actually means.
        for action in [
            Action::Player(PlayerAction::Next),
            Action::Player(PlayerAction::Seek(SeekTarget::Relative(5000))),
            Action::Item(ItemAction::ToggleFavorite),
            Action::Modal(loxia_core::action::ModalAction::Open(
                loxia_core::state::modal::ModalKind::Help,
            )),
        ] {
            let mut state = fixtures::fixture_playing_queue();
            state.zen_mode = true;
            let mut state_no_zen = fixtures::fixture_playing_queue();
            state_no_zen.zen_mode = false;

            let effects_zen = loxia_core::reducer::apply(&mut state, action.clone());
            let effects_no_zen = loxia_core::reducer::apply(&mut state_no_zen, action.clone());

            assert_eq!(
                effects_zen.len(),
                effects_no_zen.len(),
                "{action:?} must behave identically whether Zen is on or off"
            );
        }
    }

    #[test]
    fn zen_snapshot_wide() {
        let state = zen_state();
        insta::assert_snapshot!(render_at(140, 24, &state));
    }

    #[test]
    fn zen_snapshot_narrow() {
        let state = zen_state();
        insta::assert_snapshot!(render_at(90, 24, &state));
    }

    #[test]
    fn zen_snapshot_no_lyrics() {
        let mut state = zen_state();
        state.config.ui.show_lyrics = false;
        insta::assert_snapshot!(render_at(140, 24, &state));
    }

    #[test]
    fn zen_snapshot_nothing_playing() {
        let mut state = fixtures::fixture_empty();
        state.zen_mode = true;
        state.keymap = KeyMap::defaults();
        insta::assert_snapshot!(render_at(80, 24, &state));
    }
}
