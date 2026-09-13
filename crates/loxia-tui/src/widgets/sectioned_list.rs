//! The three-section (Artists/Albums/Tracks) result list shared by the Search (`07-01`) and
//! Favourites (`07-02`) tabs — extracted here so favourites doesn't duplicate it.

use loxia_core::state::nav::LoadState;
use loxia_core::state::search::{SearchResults, SearchSection};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;
use crate::text;

/// Every non-collapsed section gets at least this many rows.
const MIN_SECTION_HEIGHT: u16 = 3;
/// A section with no results collapses to a single dim line rather than being hidden.
const COLLAPSED_HEIGHT: u16 = 1;

/// `sections` names which sections this caller shows, in order. Search has no playlists to show —
/// its query never asks for any — and a permanently empty `PLAYLISTS (0)` heading there would be
/// pure noise, so callers name their own rather than the widget assuming every variant
/// (`docs/12-decisions.md`).
///
/// `focused_section: None` means nothing in this widget has focus (e.g. Search's query line does
/// instead) — every row then renders as if no section were focused, but the cursor row still
/// shows in each section's own dedicated (unfocused) selection style.
#[allow(clippy::too_many_arguments)]
pub fn render(
    f: &mut Frame,
    canvas: Rect,
    results: &SearchResults,
    load: &LoadState,
    focused_section: Option<SearchSection>,
    cursors: [usize; 4],
    sections: &[SearchSection],
    theme: &Theme,
    hits: &mut HitMap,
) {
    if canvas.width == 0 || canvas.height == 0 {
        return;
    }

    let counts: Vec<usize> = sections.iter().map(|&s| results.count(s)).collect();
    let heights = section_heights(canvas.height, &counts);

    let mut y = canvas.y;
    for (i, &section) in sections.iter().enumerate() {
        // The last section takes whatever's left over, computed directly from the canvas rather
        // than trusting `heights[2]` to have summed exactly — a render must never panic on a
        // slightly-off rounding; only the pure helper's own unit tests need to prove the sum
        // exact.
        let remaining = (canvas.y + canvas.height).saturating_sub(y);
        let h = if i + 1 == sections.len() {
            remaining
        } else {
            heights[i].min(remaining)
        };
        if h == 0 {
            break;
        }
        let area = Rect::new(canvas.x, y, canvas.width, h);
        let focused = focused_section == Some(section);
        render_section(
            f,
            area,
            section,
            results,
            load,
            cursors[section.index()],
            focused,
            theme,
            hits,
        );
        y += h;
    }
}

/// Proportional to result counts, clamped to `[MIN_SECTION_HEIGHT, available / 2]` — a section
/// with zero results is pinned to `COLLAPSED_HEIGHT` regardless (it isn't competing for
/// proportional space at all), and the remaining room is split among whichever sections actually
/// have results.
fn section_heights(available: u16, counts: &[usize]) -> Vec<u16> {
    let mut heights = vec![0u16; counts.len()];
    let nonempty: Vec<usize> = (0..counts.len()).filter(|&i| counts[i] > 0).collect();
    let collapsed_rows = COLLAPSED_HEIGHT * (counts.len() as u16 - nonempty.len() as u16);
    for (i, height) in heights.iter_mut().enumerate() {
        if counts[i] == 0 {
            *height = COLLAPSED_HEIGHT.min(available);
        }
    }
    if nonempty.is_empty() {
        return heights;
    }

    let remaining = available.saturating_sub(collapsed_rows);
    let min_h = MIN_SECTION_HEIGHT.min(remaining.max(1));
    let max_h = (available / 2).max(min_h);
    let total_count: usize = nonempty.iter().map(|&i| counts[i]).sum();

    for &i in &nonempty {
        let share = ((counts[i] as u64 * remaining as u64) / total_count.max(1) as u64) as u16;
        heights[i] = share.clamp(min_h, max_h);
    }

    // Absorb any rounding remainder within whichever non-empty section still has headroom,
    // preferring the currently-largest one — a clamped-at-`max_h` section has none, so it must
    // never be picked over one still below the ceiling.
    let mut sum: i32 = heights.iter().map(|&h| h as i32).sum();
    let target = available as i32;
    let mut guard = 0;
    while sum != target && guard <= available as i32 {
        guard += 1;
        if sum < target {
            let Some(&idx) = nonempty
                .iter()
                .filter(|&&i| heights[i] < max_h)
                .max_by_key(|&&i| heights[i])
            else {
                break;
            };
            heights[idx] += 1;
            sum += 1;
        } else {
            let Some(&idx) = nonempty
                .iter()
                .filter(|&&i| heights[i] > min_h)
                .min_by_key(|&&i| heights[i])
            else {
                break;
            };
            heights[idx] -= 1;
            sum -= 1;
        }
    }
    heights
}

fn section_label(section: SearchSection) -> &'static str {
    match section {
        SearchSection::Artists => "ARTISTS",
        SearchSection::Albums => "ALBUMS",
        SearchSection::Tracks => "TRACKS",
        SearchSection::Playlists => "PLAYLISTS",
    }
}

#[allow(clippy::too_many_arguments)]
fn render_section(
    f: &mut Frame,
    area: Rect,
    section: SearchSection,
    results: &SearchResults,
    load: &LoadState,
    cursor: usize,
    focused: bool,
    theme: &Theme,
    hits: &mut HitMap,
) {
    let count = results.count(section);

    if count == 0 {
        render_collapsed(f, area, section, results, load, theme);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, focused))
        .title(format!("{} ({count})", section_label(section)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Each section is only a few rows tall (proportional to result counts), but a search can return
    // far more results than that. Scroll the section to keep its own cursor on screen — otherwise
    // the flat `↑`/`↓` navigation (which moves through *every* result) walks the cursor onto rows
    // clipped below the visible few, which reads as "there's something there but I can't see it"
    // (`docs/12-decisions.md`).
    let start = scroll_start(count, cursor, inner.height as usize);
    match section {
        SearchSection::Artists => {
            for (i, artist) in results
                .artists
                .iter()
                .enumerate()
                .skip(start)
                .take(inner.height as usize)
            {
                let row = row_at(inner, i - start);
                render_row(f, row, &artist.name, i == cursor, focused, theme);
                hits.push(row, HitTarget::SectionedListItem { section, index: i });
            }
        }
        SearchSection::Albums => {
            for (i, album) in results
                .albums
                .iter()
                .enumerate()
                .skip(start)
                .take(inner.height as usize)
            {
                let label = match album.year {
                    Some(y) => format!("{} ({y})", album.name),
                    None => album.name.clone(),
                };
                let row = row_at(inner, i - start);
                render_row(f, row, &label, i == cursor, focused, theme);
                hits.push(row, HitTarget::SectionedListItem { section, index: i });
            }
        }
        SearchSection::Tracks => {
            for (i, track) in results
                .tracks
                .iter()
                .enumerate()
                .skip(start)
                .take(inner.height as usize)
            {
                let artist = track.artist_names.first().map(String::as_str).unwrap_or("");
                let label = if artist.is_empty() {
                    track.name.clone()
                } else {
                    format!("{artist} — {}", track.name)
                };
                let row = row_at(inner, i - start);
                render_row(f, row, &label, i == cursor, focused, theme);
                hits.push(row, HitTarget::SectionedListItem { section, index: i });
            }
        }
        SearchSection::Playlists => {
            for (i, playlist) in results
                .playlists
                .iter()
                .enumerate()
                .skip(start)
                .take(inner.height as usize)
            {
                let label = if playlist.track_count == 0 {
                    playlist.name.clone()
                } else {
                    format!("{} ({} tracks)", playlist.name, playlist.track_count)
                };
                let row = row_at(inner, i - start);
                render_row(f, row, &label, i == cursor, focused, theme);
                hits.push(row, HitTarget::SectionedListItem { section, index: i });
            }
        }
    }
}

/// The first item index to render so `cursor` stays visible when the section holds more results
/// than fit in `height` rows. Centres the cursor, clamped at both ends.
fn scroll_start(len: usize, cursor: usize, height: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    cursor.saturating_sub(height / 2).min(len - height)
}

fn row_at(inner: Rect, i: usize) -> Rect {
    Rect::new(inner.x, inner.y + i as u16, inner.width, 1)
}

fn render_row(
    f: &mut Frame,
    area: Rect,
    label: &str,
    is_cursor: bool,
    focused: bool,
    theme: &Theme,
) {
    let row_style = if is_cursor {
        style::selection(theme, focused)
    } else {
        style::style(theme, Role::Fg)
    };
    let text = text::truncate(label, area.width as usize).into_owned();
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, row_style))),
        area,
    );
}

/// A section with no results (or one whose own request failed, or still in flight) is collapsed
/// to a single dim line, never hidden entirely: a user needs to see that a category was searched
/// and came back empty (or errored, or is still loading) rather than wondering if it was searched
/// at all.
fn render_collapsed(
    f: &mut Frame,
    area: Rect,
    section: SearchSection,
    results: &SearchResults,
    load: &LoadState,
    theme: &Theme,
) {
    if area.height == 0 {
        return;
    }
    let row = Rect::new(area.x, area.y, area.width, 1);
    let label = match (load, results.error(section)) {
        (LoadState::Loading, _) => format!("{} — searching…", section_label(section)),
        (_, Some(_)) => format!("{} — error", section_label(section)),
        (_, None) => format!("{} (0)", section_label(section)),
    };
    let text = text::truncate(&label, area.width as usize).into_owned();
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style::fg(theme, Role::Dim))))
            .alignment(Alignment::Left),
        row,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_are_equal_when_all_empty() {
        let h = section_heights(30, &[0; 4]);
        assert_eq!(h, vec![1; 4]);
    }

    #[test]
    fn section_heights_proportional_within_bounds() {
        let available = 30;
        // The playlists section is empty, which is the Search tab's permanent case.
        let h = section_heights(available, &[3, 12, 47, 0]);
        let total: u16 = h.iter().sum();
        assert_eq!(total, available);
        for (i, &height) in h.iter().enumerate() {
            if height == COLLAPSED_HEIGHT && i + 1 == h.len() {
                continue; // the empty section keeps only its collapsed heading
            }
            assert!(height >= MIN_SECTION_HEIGHT.min(available));
            assert!(height <= available / 2);
        }
        // More tracks than albums than artists -> tracks tallest, artists shortest (or tied at
        // the floor).
        assert!(h[2] >= h[1]);
        assert!(h[1] >= h[0]);
    }

    #[test]
    fn scroll_start_keeps_the_cursor_visible() {
        assert_eq!(scroll_start(3, 2, 10), 0, "everything fits -> no scroll");
        assert_eq!(scroll_start(50, 0, 6), 0, "cursor at top clamps to 0");
        assert_eq!(scroll_start(50, 40, 6), 37, "cursor centred in the window");
        assert_eq!(
            scroll_start(50, 49, 6),
            44,
            "cursor at end clamps to last page"
        );
    }

    #[test]
    fn empty_section_collapses_to_one_line() {
        let h = section_heights(30, &[0, 5, 10, 0]);
        assert_eq!(h[0], COLLAPSED_HEIGHT);
        assert!(h[1] >= MIN_SECTION_HEIGHT);
        assert!(h[2] >= MIN_SECTION_HEIGHT);
        assert_eq!(h.iter().sum::<u16>(), 30);
    }
}
