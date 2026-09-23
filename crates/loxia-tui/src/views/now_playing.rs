//! Now Playing tab (`07-06`, `docs/07-ui-spec.md` §8): the queue (or history) on the left, track
//! detail and transport on the right.

use loxia_core::keymap::ActionId;
use loxia_core::state::queue::QueueSource;
use loxia_core::state::{AppState, NowPlayingSub};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;
use crate::text;

/// Below this width the right pane collapses entirely and the left pane takes the full canvas —
/// the queue is the more useful of the two when space is scarce.
const RIGHT_PANE_MIN_TOTAL_WIDTH: u16 = 100;
/// Left pane's share of the canvas once both panes fit.
const LEFT_PANE_PERCENT: u32 = 45;

pub fn render(
    f: &mut Frame,
    canvas: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    if canvas.width == 0 || canvas.height == 0 {
        return Vec::new();
    }

    if canvas.width < RIGHT_PANE_MIN_TOTAL_WIDTH {
        render_left_pane(f, canvas, state, theme, hits);
        return Vec::new();
    }

    let left_w = ((u32::from(canvas.width) * LEFT_PANE_PERCENT) / 100) as u16;
    let left_area = Rect::new(canvas.x, canvas.y, left_w, canvas.height);
    let right_area = Rect::new(
        canvas.x + left_w,
        canvas.y,
        canvas.width - left_w,
        canvas.height,
    );
    render_left_pane(f, left_area, state, theme, hits);
    render_right_pane(f, right_area, state, theme, hits, art)
}

fn format_mmss(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// The first visible row: the offset the reducer stores, clamped into range.
fn scroll_start(len: usize, cursor: usize, height: usize, stored: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    let max_start = len - height;
    let start = stored.min(max_start);
    if cursor < start {
        cursor
    } else if cursor >= start + height {
        (cursor + 1 - height).min(max_start)
    } else {
        start
    }
}

/// The queue pane's title, honouring the active sub-view (`Queue` vs `History`) and naming an
/// active instant mix when the queue is still that mix.
fn left_pane_title(state: &AppState, width: u16) -> Line<'static> {
    let sub = state.nav.now_playing_sub;
    let (queue_hint, history_hint) = (
        loxia_core::keymap::KeyMap::hint_for(ActionId::ToggleQueueHistory),
        loxia_core::keymap::KeyMap::hint_for(ActionId::ToggleQueueHistory),
    );
    let _ = width;
    match sub {
        NowPlayingSub::Queue => {
            let mix_suffix = if state.queue.source == QueueSource::InstantMix {
                " (mix)".to_string()
            } else {
                String::new()
            };
            Line::from(format!("PLAY QUEUE{mix_suffix}  [{history_hint}] History"))
        }
        NowPlayingSub::History => Line::from(format!("HISTORY  [{queue_hint}] Queue")),
    }
}

fn render_left_pane(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let title = left_pane_title(state, area.width);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(style::style(theme, Role::Fg))
        .border_style(style::style(theme, Role::Border));
    let inner = block.inner(area);
    f.render_widget(block, area);

    match state.nav.now_playing_sub {
        NowPlayingSub::Queue => render_queue_rows(f, inner, state, theme, hits),
        NowPlayingSub::History => render_history_rows(f, inner, state, theme),
    }
}

fn render_queue_rows(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if state.queue.play_order.is_empty() {
        let hint = loxia_core::keymap::KeyMap::hint_for(ActionId::PlayItem);
        let msg1 = format!("queue is empty — press {hint} on an album");
        let msg2 = "to start".to_string();
        let row1 = area.height / 2;
        f.render_widget(
            Paragraph::new(msg1).alignment(Alignment::Center),
            Rect::new(area.x, area.y + row1.saturating_sub(0), area.width, 1),
        );
        if area.height > row1 + 1 {
            f.render_widget(
                Paragraph::new(msg2).alignment(Alignment::Center),
                Rect::new(area.x, area.y + row1 + 1, area.width, 1),
            );
        }
        return;
    }

    let height = area.height as usize;
    let start = scroll_start(
        state.queue.play_order.len(),
        state.queue.position,
        height,
        state.nav.queue_scroll_offset,
    );

    let mut lines: Vec<Line> = Vec::new();
    for (i, entry) in state
        .queue
        .play_order
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
    {
        let marker = if i == state.queue.position { "▸ " } else { "  " };
        let text = format!("{marker}{}", entry.title());
        let style = if i == state.queue.position {
            style::style(theme, Role::Accent).add_modifier(Modifier::BOLD)
        } else {
            style::style(theme, Role::Fg)
        };
        lines.push(Line::from(Span::styled(text, style)));
        let row_y = area.y + (lines.len() as u16 - 1);
        hits.push(
            Rect::new(area.x, row_y, area.width, 1),
            HitTarget::QueueRow(i),
        );
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_history_rows(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if state.queue.history.is_empty() {
        let msg = "no history yet";
        let row = area.height / 2;
        f.render_widget(
            Paragraph::new(msg).alignment(Alignment::Center),
            Rect::new(area.x, area.y + row, area.width, 1),
        );
        return;
    }

    let height = area.height as usize;
    // History is displayed most-recent-first.
    let entries: Vec<_> = state.queue.history.iter().rev().collect();
    let mut lines: Vec<Line> = Vec::new();
    for entry in entries.iter().take(height) {
        let (hour, minute) = loxia_core::local_hour_minute(entry.played_at);
        let dur = format_mmss(entry.duration);
        let text = format!(
            "• {hour:02}:{minute:02}  {} — {}{}",
            entry.artist(),
            entry.title(),
            " ".repeat(
                (area.width as usize)
                    .saturating_sub(2 + 8 + entry.artist().len() + 3 + entry.title().len() + dur.len())
            )
        );
        let mut line = text;
        line.push_str(&dur);
        lines.push(Line::from(Span::styled(
            line,
            style::style(theme, Role::Fg),
        )));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_right_pane(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
    art: &mut crate::widgets::album_art::Art<'_>,
) -> Vec<loxia_core::effect::Effect> {
    let block = Block::default()
        .borders(Borders::ALL)
        .style(style::style(theme, Role::Fg))
        .border_style(style::style(theme, Role::Border));
    let inner = block.inner(area);
    f.render_widget(block, area);

    match &state.player.now_playing {
        Some(track) => {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(track.title()));
            lines.push(Line::from(track.artist()));
            lines.push(Line::from(format!(
                "Album: {} ({}) | Genre: {}",
                track.album_name(),
                track.year().map(|y| y.to_string()).unwrap_or_default(),
                track.genre().unwrap_or_else(|| "—".to_string())
            )));
            f.render_widget(
                Paragraph::new(lines),
                Rect::new(inner.x, inner.y, inner.width, 3.min(inner.height)),
            );

            if inner.height > 4 {
                let art_area = Rect::new(
                    inner.x,
                    inner.y + 4,
                    inner.width,
                    inner.height.saturating_sub(4),
                );
                art.render(f, art_area, theme, hits, track.image_url());
            }
        }
        None => {
            let msg = "nothing playing";
            let row = inner.height / 2;
            f.render_widget(
                Paragraph::new(msg).alignment(Alignment::Center),
                Rect::new(inner.x, inner.y + row, inner.width, 1.min(inner.height)),
            );
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::state::queue::HistoryEntry;
    use loxia_core::test_support::fixtures;

    fn render_at(width: u16, height: u16, state: &AppState) -> (ratatui::buffer::Buffer, HitMap) {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        let theme = Theme::default();
        let mut hits = HitMap::default();
        let mut art = crate::widgets::album_art::Art::disabled();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme, &mut hits, &mut art);
            })
            .expect("draw");
        (terminal.backend().buffer().clone(), hits)
    }

    #[test]
    fn now_playing_snapshot_history() {
        let mut state = AppState::default();
        state.nav.now_playing_sub = NowPlayingSub::History;
        state.clock = fixtures::fixed_clock_hm(23, 15);
        for i in 0..3 {
            state.queue.history.push(HistoryEntry::fixture_boy_harsher(
                i,
                fixtures::fixed_clock_hm(23, 13 + i as i64),
            ));
        }
        state.player.now_playing = Some(fixtures::fixture_track_4());
        insta::assert_debug_snapshot!(render_at(120, 30, &state).0);
    }
}
