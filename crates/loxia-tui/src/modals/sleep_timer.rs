//! Sleep and shutdown timer modal (`10-07`, `design_overview` §3.6): arms `player.sleep_timer`
//! with a radio-button trigger and two independent option checkboxes.

use loxia_core::state::AppState;
use loxia_core::state::modal::Modal;
use loxia_core::state::player::{SleepTimer, SleepTrigger};
use loxia_core::state::queue::RepeatMode;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::time::Duration;

use crate::hit::HitMap;
use crate::style;
use crate::widgets::player_bar::format_time;

const MODAL_WIDTH: u16 = 78;
const CONTENT_ROWS: u16 = 11;

fn radio_glyph(selected: bool) -> &'static str {
    if selected { "(\u{2022})" } else { "( )" }
}

fn checkbox_glyph(checked: bool) -> &'static str {
    if checked { "[X]" } else { "[ ]" }
}

/// The static description shown once a timer is already armed — matches `widgets::header`'s own
/// badge text for the same data (`09-05`), rather than a live decrementing countdown: neither this
/// modal nor `loxia-tui` in general has a reason to depend on `jiff` just to recompute one, and the
/// header's own convention ("the configured length/description", not a live tick) is already
/// established for exactly this field.
fn armed_summary(timer: &SleepTimer, state: &AppState) -> String {
    match timer.trigger {
        SleepTrigger::Duration(d) => format!("{} minutes", d.as_secs() / 60),
        SleepTrigger::EndOfTrack => "end of current track".to_string(),
        SleepTrigger::EndOfQueue => {
            let remaining = queue_remaining(state);
            format!("end of queue ({remaining} left)")
        }
    }
}

fn queue_remaining(state: &AppState) -> usize {
    state
        .queue
        .play_order
        .len()
        .saturating_sub(state.queue.position + 1)
}

fn track_remaining(state: &AppState) -> Duration {
    state.player.duration.saturating_sub(state.player.position)
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, _hits: &mut HitMap) {
    let Some(Modal::SleepTimer {
        trigger,
        fade_out,
        quit_after,
        field_cursor,
    }) = &state.modal
    else {
        return;
    };
    if area.width < 3 || area.height < 3 {
        return;
    }

    let has_current_track = state.queue.current().is_some();
    let show_repeat_note =
        *trigger == SleepTrigger::EndOfQueue && state.queue.repeat == RepeatMode::All;
    let content_rows = CONTENT_ROWS + u16::from(show_repeat_note);

    let title = match &state.player.sleep_timer {
        Some(timer) => format!("SLEEP TIMER \u{2014} {}", armed_summary(timer, state)),
        None => "SLEEP & SHUTDOWN TIMER".to_string(),
    };

    let width = MODAL_WIDTH.min(area.width).max(1);
    let height = (content_rows + 2).min(area.height).max(1);
    let modal_area = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .style(style::modal_surface(theme))
        .border_style(style::border(theme, true))
        .title(title);
    let inner = block.inner(modal_area);
    // Wipe whatever the view underneath drew before painting the modal — a `Block` only paints its
    // border, so without this the canvas text showed *through* the modal body (seen in the field
    // with the sort menu over Now Playing). `docs/12-decisions.md`.
    f.render_widget(ratatui::widgets::Clear, modal_area);
    f.render_widget(block, modal_area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let end_y = inner.y + inner.height;
    let mut y = inner.y;
    let fg = style::style(theme, Role::Fg);
    let dim = style::fg(theme, Role::Dim);
    let accent = style::fg(theme, Role::Accent);

    let row_style = |focused: bool, disabled: bool| {
        if disabled {
            dim
        } else if focused {
            accent
        } else {
            fg
        }
    };

    y = draw_line(f, inner, y, end_y, "Stop playback after:", dim);

    // The three duration triggers share one visual row (`design_overview`'s own layout), each its
    // own field-cursor stop (`0..=2`) for navigation purposes.
    if y < end_y {
        let mut spans = Vec::new();
        for (idx, (label, secs)) in [
            ("15 Minutes", 900u64),
            ("30 Minutes", 1800),
            ("60 Minutes", 3600),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = *trigger == SleepTrigger::Duration(Duration::from_secs(secs));
            let marker = if *field_cursor == idx {
                "\u{25b6} "
            } else {
                "  "
            };
            spans.push(Span::styled(
                format!("{marker}{} {label}   ", radio_glyph(selected)),
                row_style(*field_cursor == idx, false),
            ));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }

    // Row 3: End of Current Track.
    if y < end_y {
        let marker = if *field_cursor == 3 {
            "\u{25b6} "
        } else {
            "  "
        };
        let selected = *trigger == SleepTrigger::EndOfTrack;
        let text = if has_current_track {
            format!(
                "{marker}{} End of Current Track ({})",
                radio_glyph(selected),
                format_time(track_remaining(state))
            )
        } else {
            format!("{marker}{} End of Current Track", radio_glyph(selected))
        };
        draw_line(
            f,
            inner,
            y,
            end_y,
            &text,
            row_style(*field_cursor == 3, !has_current_track),
        );
        y += 1;
    }

    // Row 4: End of Active Queue.
    if y < end_y {
        let marker = if *field_cursor == 4 {
            "\u{25b6} "
        } else {
            "  "
        };
        let selected = *trigger == SleepTrigger::EndOfQueue;
        let remaining = queue_remaining(state);
        let text = format!(
            "{marker}{} End of Active Queue ({remaining} tracks remaining)",
            radio_glyph(selected)
        );
        draw_line(
            f,
            inner,
            y,
            end_y,
            &text,
            row_style(*field_cursor == 4, !has_current_track),
        );
        y += 1;
    }

    if show_repeat_note {
        y = draw_line(
            f,
            inner,
            y,
            end_y,
            "    (repeat will be disabled so this can fire)",
            dim,
        );
    }

    y = draw_line(f, inner, y, end_y, "", fg);
    y = draw_line(f, inner, y, end_y, "Options:", dim);

    if y < end_y {
        let marker = if *field_cursor == 5 {
            "\u{25b6} "
        } else {
            "  "
        };
        let text = format!(
            "{marker}{} Fade out audio smoothly over the last 10 seconds",
            checkbox_glyph(*fade_out)
        );
        draw_line(
            f,
            inner,
            y,
            end_y,
            &text,
            row_style(*field_cursor == 5, false),
        );
        y += 1;
    }
    if y < end_y {
        let marker = if *field_cursor == 6 {
            "\u{25b6} "
        } else {
            "  "
        };
        let text = format!(
            "{marker}{} Close loxia when finished",
            checkbox_glyph(*quit_after)
        );
        draw_line(
            f,
            inner,
            y,
            end_y,
            &text,
            row_style(*field_cursor == 6, false),
        );
        y += 1;
    }

    y = draw_line(f, inner, y, end_y, "", fg);
    let footer = "[ Enter ] Start    [ d ] Disarm    [ Esc ] Cancel";
    draw_line(f, inner, y, end_y, footer, dim);
}

fn draw_line(
    f: &mut Frame,
    inner: Rect,
    y: u16,
    end_y: u16,
    text: &str,
    style: ratatui::style::Style,
) -> u16 {
    if y < end_y {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text.to_string(), style))),
            Rect::new(inner.x, y, inner.width, 1),
        );
    }
    y + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn modal(
        trigger: SleepTrigger,
        fade_out: bool,
        quit_after: bool,
        field_cursor: usize,
    ) -> Modal {
        Modal::SleepTimer {
            trigger,
            fade_out,
            quit_after,
            field_cursor,
        }
    }

    fn draw_at(w: u16, h: u16, state: &AppState) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme, &mut hits)
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    #[test]
    fn triggers_are_mutually_exclusive() {
        let mut state = fixtures::fixture_playing_queue();
        state.modal = Some(modal(
            SleepTrigger::Duration(Duration::from_secs(1800)),
            true,
            false,
            1,
        ));
        let rendered = draw_at(90, 20, &state);
        // Only the 30-minute row is filled; the others (including the two playback triggers) are
        // hollow.
        assert!(rendered.contains("(\u{2022}) 30 Minutes"));
        assert!(rendered.contains("( ) 15 Minutes"));
        assert!(rendered.contains("( ) 60 Minutes"));
        assert!(rendered.contains("( ) End of Current Track"));
        assert!(rendered.contains("( ) End of Active Queue"));
    }

    #[test]
    fn options_are_independent() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal(
            SleepTrigger::Duration(Duration::from_secs(900)),
            true,
            false,
            0,
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("[X] Fade out"));
        assert!(rendered.contains("[ ] Close loxia"));
    }

    #[test]
    fn radio_and_checkbox_glyphs_differ() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal(
            SleepTrigger::Duration(Duration::from_secs(900)),
            true,
            true,
            0,
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("(\u{2022})"));
        assert!(rendered.contains("[X]"));
        assert!(!rendered.contains("[\u{2022}]"));
        assert!(!rendered.contains("(X)"));
    }

    #[test]
    fn queue_row_shows_live_remaining_count() {
        let mut state = fixtures::fixture_playing_queue();
        // 10 entries, position 3 (0-indexed) => 6 remaining.
        state.modal = Some(modal(SleepTrigger::EndOfQueue, true, false, 4));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("(6 tracks remaining)"));
    }

    #[test]
    fn track_row_shows_remaining_time() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.duration = Duration::from_secs(200);
        state.player.position = Duration::from_secs(50);
        state.modal = Some(modal(SleepTrigger::EndOfTrack, true, false, 3));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains(&format_time(Duration::from_secs(150))));
    }

    #[test]
    fn repeat_all_note_shown_inline_before_commit() {
        let mut state = fixtures::fixture_playing_queue();
        state.queue.repeat = RepeatMode::All;
        state.modal = Some(modal(SleepTrigger::EndOfQueue, true, false, 4));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("repeat will be disabled"));

        state.queue.repeat = RepeatMode::Off;
        let rendered = draw_at(90, 20, &state);
        assert!(!rendered.contains("repeat will be disabled"));
    }

    #[test]
    fn already_armed_shows_remaining_in_title_and_preselects() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.sleep_timer = Some(SleepTimer {
            trigger: SleepTrigger::Duration(Duration::from_secs(1800)),
            fade_out: true,
            quit_after: false,
            armed_at: loxia_core::Timestamp::now(),
            armed_entry: None,
            pre_fade_volume: None,
        });
        state.modal = Some(modal(
            SleepTrigger::Duration(Duration::from_secs(1800)),
            true,
            false,
            1,
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("SLEEP TIMER \u{2014} 30 minutes"));
        assert!(rendered.contains("(\u{2022}) 30 Minutes"));
    }

    #[test]
    fn playback_triggers_disabled_when_nothing_playing() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal(
            SleepTrigger::Duration(Duration::from_secs(900)),
            true,
            false,
            0,
        ));
        assert!(state.queue.current().is_none());
        // Doesn't panic and doesn't show a track-remaining time when nothing is loaded.
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("End of Current Track"));
        assert!(!rendered.contains("End of Current Track ("));
    }

    #[test]
    fn does_not_render_for_other_modal_kinds() {
        let state_with_help = {
            let mut state = fixtures::fixture_empty();
            state.modal = Some(Modal::Help {
                context: loxia_core::keymap::InputContext::Normal,
                scroll: 0,
            });
            state
        };
        let rendered = draw_at(90, 20, &state_with_help);
        assert!(!rendered.contains("SLEEP"));
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal(
            SleepTrigger::Duration(Duration::from_secs(900)),
            true,
            false,
            0,
        ));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn sleep_timer_snapshot() {
        let mut state = fixtures::fixture_playing_queue();
        state.modal = Some(modal(SleepTrigger::EndOfQueue, true, false, 4));
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn sleep_timer_snapshot_armed() {
        let mut state = fixtures::fixture_playing_queue();
        state.player.sleep_timer = Some(SleepTimer {
            trigger: SleepTrigger::EndOfTrack,
            fade_out: false,
            quit_after: true,
            armed_at: loxia_core::Timestamp::now(),
            armed_entry: None,
            pre_fade_volume: None,
        });
        state.modal = Some(modal(SleepTrigger::EndOfTrack, false, true, 3));
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn sleep_timer_snapshot_nothing_playing() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal(
            SleepTrigger::Duration(Duration::from_secs(900)),
            true,
            false,
            0,
        ));
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }
}
