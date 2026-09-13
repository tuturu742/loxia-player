//! Top status header bar (`docs/07-ui-spec.md` §3):
//! `loxia │ <server> │ <active tab> │ <badges> │ <clock>`.

use loxia_core::state::AppState;
use loxia_core::state::player::SleepTrigger;
use loxia_core::state::queue::RepeatMode;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::style;
use crate::text;
use crate::widgets::sidebar::tab_label;

/// `⧗`, pinned to its text presentation — see `text::narrow_glyph`.
fn timer_glyph() -> String {
    text::narrow_glyph('\u{23F1}')
}

/// One badge: its text and the role it's styled in, in the order `docs/07-ui-spec.md` §3 /
/// this task's own spec text give. Badges are never dropped for width, unlike the left-hand
/// segments.
fn badges(state: &AppState) -> Vec<(String, Role)> {
    let mut out = Vec::new();

    if state.connectivity != loxia_core::state::Connectivity::Online {
        out.push(("OFFLINE".to_string(), Role::Error));
    }
    if state.downloads_active > 0 {
        out.push((format!("↓{}", state.downloads_active), Role::Accent));
    }
    if state.queue.shuffled {
        out.push(("⇄".to_string(), Role::Accent));
    }
    match state.queue.repeat {
        RepeatMode::Off => {}
        RepeatMode::All => out.push(("🔁".to_string(), Role::Accent)),
        RepeatMode::One => out.push(("🔂".to_string(), Role::Accent)),
    }
    if let Some(timer) = &state.player.sleep_timer {
        // `09-05`: `EndOfTrack`/`EndOfQueue` were previously collapsed into a single bare "⧗" —
        // this task's own spec gives each its own text ("⧗track", "⧗queue (3)" with the
        // remaining-entry count), which is what actually tells the two apart in the header.
        let text = match timer.trigger {
            SleepTrigger::Duration(d) => format!("{}{}m", timer_glyph(), d.as_secs() / 60),
            SleepTrigger::EndOfTrack => format!("{}track", timer_glyph()),
            SleepTrigger::EndOfQueue => {
                let remaining = state
                    .queue
                    .play_order
                    .len()
                    .saturating_sub(state.queue.position + 1);
                format!("{}queue ({remaining})", timer_glyph())
            }
        };
        out.push((text, Role::Accent));
    }
    if !state.config_warnings.is_empty() {
        out.push((
            format!(
                "{}{}",
                text::narrow_glyph('\u{25B3}'),
                state.config_warnings.len()
            ),
            Role::Warning,
        ));
    }

    out
}

/// Reads `state.clock` (set from `Tick`'s own timestamp, `04-05`) — never `Timestamp::now()`.
/// Two renders of the same state must be byte-identical, which a direct system-clock read would
/// break.
fn format_clock(state: &AppState) -> String {
    let (hour, minute) = loxia_core::local_hour_minute(state.clock);
    format!("{hour:02}:{minute:02}")
}

/// The `loxia │ server │ tab` segment, honouring the elision flags — a dropped segment removes
/// both its text and its separator.
fn left_segment(state: &AppState, show_server: bool, show_tab: bool) -> String {
    let mut parts = vec!["loxia".to_string()];
    if show_server {
        let server = state
            .server
            .server_name
            .clone()
            .unwrap_or_else(|| "no server".to_string());
        parts.push(server);
    }
    if show_tab {
        parts.push(tab_label(state.nav.active_tab).to_string());
    }
    parts.join(" │ ")
}

/// The mouse-reachable help button, sitting immediately left of the clock. Labelled with the key
/// that does the same thing from the keyboard, so the button teaches the binding.
const HELP_LABEL: &str = "[?]";

/// The right-hand cluster's spans, in order — badges, then the help button, then the clock, each
/// pair separated by ` │ `. Also returns the help button's own offset within the cluster (`None`
/// when it was elided), which is all the caller needs to turn it into a clickable rect: building
/// the spans and measuring the button in one pass is what keeps the two from drifting apart.
fn right_spans(
    state: &AppState,
    theme: &Theme,
    badge_list: &[(String, Role)],
    show_help: bool,
    show_clock: bool,
) -> (Vec<Span<'static>>, Option<usize>) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut help_offset = None;
    let mut width = 0usize;
    let push = |spans: &mut Vec<Span<'static>>, width: &mut usize, span: Span<'static>| {
        *width += text::width(&span.content);
        spans.push(span);
    };

    for (i, (text, role)) in badge_list.iter().enumerate() {
        if i > 0 {
            push(&mut spans, &mut width, Span::raw(" "));
        }
        push(
            &mut spans,
            &mut width,
            Span::styled(text.clone(), style::fg(theme, *role)),
        );
    }

    let separate = |spans: &mut Vec<Span<'static>>, width: &mut usize| {
        if !spans.is_empty() {
            let sep = Span::styled(" │ ", style::fg(theme, Role::Border));
            *width += text::width(&sep.content);
            spans.push(sep);
        }
    };

    if show_help {
        separate(&mut spans, &mut width);
        help_offset = Some(width);
        push(
            &mut spans,
            &mut width,
            Span::styled(HELP_LABEL, style::fg(theme, Role::Accent)),
        );
    }
    if show_clock {
        separate(&mut spans, &mut width);
        push(
            &mut spans,
            &mut width,
            Span::styled(format_clock(state), style::style(theme, Role::Fg)),
        );
    }

    (spans, help_offset)
}

fn right_width(
    state: &AppState,
    theme: &Theme,
    badge_list: &[(String, Role)],
    show_help: bool,
    show_clock: bool,
) -> usize {
    right_spans(state, theme, badge_list, show_help, show_clock)
        .0
        .iter()
        .map(|s| text::width(&s.content))
        .sum()
}

pub fn render(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut crate::hit::HitMap,
) {
    let width = area.width as usize;
    let badge_list = badges(state);

    // Elision order: clock, then the help button, then server name, then active tab. Badges are
    // never dropped, even if the result overflows once everything else is already gone.
    let mut show_clock = true;
    let mut show_help = true;
    let mut show_server = true;
    let mut show_tab = true;
    loop {
        let left = left_segment(state, show_server, show_tab);
        let right_w = right_width(state, theme, &badge_list, show_help, show_clock);
        let sep = if left.is_empty() || right_w == 0 {
            0
        } else {
            3
        };
        if text::width(&left) + sep + right_w <= width {
            break;
        }
        if show_clock {
            show_clock = false;
        } else if show_help {
            show_help = false;
        } else if show_server {
            show_server = false;
        } else if show_tab {
            show_tab = false;
        } else {
            break;
        }
    }

    let left = left_segment(state, show_server, show_tab);
    let (right, help_offset) = right_spans(state, theme, &badge_list, show_help, show_clock);
    let right_w: usize = right.iter().map(|s| text::width(&s.content)).sum();
    let has_right = !right.is_empty();
    let left_sep_w = if !left.is_empty() && has_right { 3 } else { 0 };
    let gap = width.saturating_sub(text::width(&left) + left_sep_w + right_w);

    let right_start = text::width(&left) + left_sep_w + gap;
    if let Some(offset) = help_offset {
        let x = area.x.saturating_add((right_start + offset) as u16);
        hits.push(
            Rect::new(x, area.y, text::width(HELP_LABEL) as u16, 1),
            crate::hit::HitTarget::HelpButton,
        );
    }

    let mut spans = vec![Span::styled(left, style::style(theme, Role::Fg))];
    if left_sep_w > 0 {
        spans.push(Span::styled(" │ ", style::fg(theme, Role::Border)));
    }
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::{HitMap, HitTarget};
    use loxia_core::config::ConfigWarning;
    use loxia_core::state::player::{SleepTimer, SleepTrigger};
    use loxia_core::state::{Connectivity, ServerSession};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(width: u16, state: &AppState) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state, &theme, &mut crate::hit::HitMap::default())
            })
            .unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    fn base_state() -> AppState {
        AppState {
            clock: loxia_core::Timestamp::from_second(1_700_000_000).unwrap(),
            server: ServerSession {
                server_name: Some("My Server".to_string()),
                ..Default::default()
            },
            ..AppState::default()
        }
    }

    #[test]
    fn header_badges_appear_only_when_active() {
        let mut state = base_state();
        assert!(!render_at(120, &state).contains("OFFLINE"));
        state.connectivity = Connectivity::Offline;
        assert!(render_at(120, &state).contains("OFFLINE"));

        let mut state = base_state();
        assert!(!render_at(120, &state).contains('↓'));
        state.downloads_active = 3;
        assert!(render_at(120, &state).contains("↓3"));

        let mut state = base_state();
        assert!(!render_at(120, &state).contains('⇄'));
        state.queue.shuffled = true;
        assert!(render_at(120, &state).contains('⇄'));

        let mut state = base_state();
        state.queue.repeat = RepeatMode::All;
        assert!(render_at(120, &state).contains("🔁"));

        let mut state = base_state();
        assert!(!render_at(120, &state).contains('\u{23F1}'));
        state.player.sleep_timer = Some(SleepTimer {
            trigger: SleepTrigger::Duration(std::time::Duration::from_secs(30 * 60)),
            fade_out: true,
            quit_after: false,
            armed_at: state.clock,
            armed_entry: None,
            pre_fade_volume: None,
        });
        assert!(render_at(120, &state).contains(&format!("{}30m", timer_glyph())));

        let mut state = base_state();
        assert!(!render_at(120, &state).contains('\u{25B3}'));
        state.config_warnings.push(ConfigWarning {
            field: "ui.theme".to_string(),
            message: "bad".to_string(),
            severity: loxia_core::config::Severity::Warning,
        });
        assert!(render_at(120, &state).contains(&format!("{}1", text::narrow_glyph('\u{25B3}'))));
    }

    #[test]
    fn badge_text_per_trigger() {
        let timer = |trigger| SleepTimer {
            trigger,
            fade_out: true,
            quit_after: false,
            armed_at: loxia_core::Timestamp::from_second(1_700_000_000).unwrap(),
            armed_entry: None,
            pre_fade_volume: None,
        };

        let mut state = base_state();
        state.player.sleep_timer = Some(timer(SleepTrigger::Duration(
            std::time::Duration::from_secs(23 * 60),
        )));
        assert!(render_at(120, &state).contains(&format!("{}23m", timer_glyph())));

        let mut state = base_state();
        state.player.sleep_timer = Some(timer(SleepTrigger::EndOfTrack));
        assert!(render_at(120, &state).contains(&format!("{}track", timer_glyph())));

        let mut state = base_state();
        // 5 entries, playing the 2nd (index 1) — 3 entries remain after it.
        state.queue.play_order = (0..5).collect();
        state.queue.position = 1;
        state.player.sleep_timer = Some(timer(SleepTrigger::EndOfQueue));
        assert!(render_at(120, &state).contains(&format!("{}queue (3)", timer_glyph())));
    }

    #[test]
    fn header_elides_clock_first_then_server() {
        let state = base_state();
        let clock = format_clock(&state);

        let wide = render_at(120, &state);
        assert!(wide.contains(&clock));
        assert!(wide.contains("My Server"));

        // Narrow enough to drop the clock but keep the server name ("loxia │ My Server │ Now
        // Playing" alone is 31 cells; add the clock back and it's 39).
        let medium = render_at(35, &state);
        assert!(!medium.contains(&clock));
        assert!(medium.contains("My Server"));

        // Narrower still: the server name goes too, but the active tab survives ("loxia │ Now
        // Playing" is 19 cells).
        let narrow = render_at(19, &state);
        assert!(!narrow.contains("My Server"));
        assert!(narrow.contains("Now Playing"));
    }

    /// The button must be clickable where it is actually drawn — the two are computed in one pass
    /// precisely so they cannot drift, and this is what proves it. Checked with badges present,
    /// since badges shift the whole right-hand cluster.
    #[test]
    fn help_button_hit_rect_matches_where_it_is_drawn() {
        for (label, mut state) in [("no badges", base_state()), ("badges", base_state())] {
            if label == "badges" {
                state.connectivity = Connectivity::Offline;
                state.downloads_active = 2;
            }

            let mut hits = HitMap::default();
            let backend = TestBackend::new(100, 1);
            let mut terminal = Terminal::new(backend).unwrap();
            let theme = Theme::default();
            terminal
                .draw(|f| {
                    let area = f.area();
                    render(f, area, &state, &theme, &mut hits)
                })
                .unwrap();
            let rendered = format!("{:?}", terminal.backend().buffer());

            let drawn_at = rendered
                .find(HELP_LABEL)
                .map(|byte| rendered[..byte].chars().count())
                // The buffer debug wraps each row in `"`, offsetting every column by one.
                .map(|col| col - rendered[..].find('"').map(|q| q + 1).unwrap_or(0))
                .expect("the help button is drawn at this width");

            let Some(&HitTarget::HelpButton) = hits.hit(drawn_at as u16, 0) else {
                panic!("{label}: no help hit region at column {drawn_at}, where it is drawn");
            };
        }
    }

    /// The clock is the first thing dropped when the header runs out of room; the help button
    /// outlives it but still yields before the server name.
    #[test]
    fn help_button_elides_after_the_clock_but_before_the_server() {
        let state = base_state();
        let clock = format_clock(&state);

        let wide = render_at(120, &state);
        assert!(wide.contains(&clock) && wide.contains(HELP_LABEL));

        let medium = render_at(38, &state);
        assert!(!medium.contains(&clock), "the clock should go first");
        assert!(medium.contains(HELP_LABEL));
        assert!(medium.contains("My Server"));

        let narrow = render_at(19, &state);
        assert!(!narrow.contains(HELP_LABEL));
        assert!(narrow.contains("Now Playing"));
    }

    /// Sits immediately left of the clock, which is where the user asked for it.
    #[test]
    fn help_button_is_left_of_the_clock() {
        let state = base_state();
        let rendered = render_at(120, &state);
        let help = rendered.find(HELP_LABEL).expect("help button drawn");
        let clock = rendered
            .find(&format_clock(&state))
            .expect("clock rendered");
        assert!(help < clock, "the help button belongs left of the clock");
    }

    #[test]
    fn header_snapshot_offline_with_downloads() {
        let mut state = base_state();
        state.connectivity = Connectivity::Offline;
        state.downloads_active = 2;
        insta::assert_snapshot!(render_at(100, &state));
    }

    #[test]
    fn header_uses_state_clock_not_system_clock() {
        let state = base_state();
        let first = render_at(100, &state);
        let second = render_at(100, &state);
        assert_eq!(first, second);
    }
}
