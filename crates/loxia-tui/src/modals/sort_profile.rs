//! Sort profile picker modal (`10-09`, `docs/02-data-model.md` §8): `o` picks a sort profile and
//! applies it to the active queue or the focused browse column.

use loxia_core::config::{Direction, SortField, SortProfile};
use loxia_core::state::AppState;
use loxia_core::state::modal::{Modal, SortApplyTarget};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::HitMap;
use crate::style;

const MODAL_WIDTH: u16 = 78;

fn field_name(field: SortField) -> &'static str {
    match field {
        SortField::Name => "name",
        SortField::Artist => "artist",
        SortField::AlbumArtist => "album_artist",
        SortField::Album => "album",
        SortField::Year => "year",
        SortField::TrackNumber => "track_number",
        SortField::Genre => "genre",
        SortField::DateAdded => "date_added",
    }
}

fn direction_arrow(direction: Direction) -> char {
    match direction {
        Direction::Asc => '\u{2191}',
        Direction::Desc => '\u{2193}',
    }
}

fn rule_chain(profile: &SortProfile) -> String {
    profile
        .rules
        .iter()
        .map(|r| format!("{} {}", field_name(r.field), direction_arrow(r.direction)))
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, _hits: &mut HitMap) {
    let Some(Modal::SortProfile {
        profiles,
        cursor,
        target,
        ..
    }) = &state.modal
    else {
        return;
    };
    if area.width < 3 || area.height < 3 {
        return;
    }

    let active_name = state.queue.sort_profile.as_deref();
    let body_rows = if profiles.is_empty() {
        1
    } else {
        profiles.len() * 2
    };
    // apply-to row, blank, body, blank, (no-profile-active note, if applicable), footer — 4 rows
    // always present (apply-to, both blanks, footer) plus the conditional ones.
    let content_rows = 4 + body_rows + usize::from(active_name.is_none());
    let width = MODAL_WIDTH.min(area.width).max(1);
    let height = ((content_rows + 2) as u16).min(area.height).max(1);
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
        .title("SORT PROFILE");
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

    let queue_radio = if *target == SortApplyTarget::Queue {
        "(\u{2022})"
    } else {
        "( )"
    };
    let column_radio = if *target == SortApplyTarget::Column {
        "(\u{2022})"
    } else {
        "( )"
    };
    y = draw_line(
        f,
        inner,
        y,
        end_y,
        &format!("Apply to: {queue_radio} Active Queue   {column_radio} Current Column"),
        fg,
    );
    y = draw_line(f, inner, y, end_y, "", fg);

    if profiles.is_empty() {
        y = draw_line(
            f,
            inner,
            y,
            end_y,
            "no sort profiles \u{2014} press {e} to create one",
            dim,
        );
    } else {
        for (idx, profile) in profiles.iter().enumerate() {
            let marker = if idx == *cursor { "\u{25b6} " } else { "  " };
            let bullet = if active_name == Some(profile.name.as_str()) {
                "\u{2022} "
            } else {
                "  "
            };
            let name_style = if idx == *cursor { accent } else { fg };
            y = draw_line(
                f,
                inner,
                y,
                end_y,
                &format!("{marker}{bullet}{}", profile.name),
                name_style,
            );
            y = draw_line(
                f,
                inner,
                y,
                end_y,
                &format!("    {}", rule_chain(profile)),
                dim,
            );
        }
    }

    y = draw_line(f, inner, y, end_y, "", fg);
    if active_name.is_none() {
        y = draw_line(f, inner, y, end_y, "no profile active", dim);
    }
    draw_line(
        f,
        inner,
        y,
        end_y,
        "[ Enter ] Apply    [ e ] Edit profiles    [ Esc ] Cancel",
        dim,
    );
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
    use loxia_core::config::SortRule;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn profile(name: &str, rules: Vec<SortRule>) -> SortProfile {
        SortProfile {
            name: name.to_string(),
            rules,
        }
    }

    fn chronological_discog() -> SortProfile {
        profile(
            "chronological_discog",
            vec![
                SortRule {
                    field: SortField::AlbumArtist,
                    direction: Direction::Asc,
                },
                SortRule {
                    field: SortField::Year,
                    direction: Direction::Asc,
                },
                SortRule {
                    field: SortField::Album,
                    direction: Direction::Asc,
                },
                SortRule {
                    field: SortField::TrackNumber,
                    direction: Direction::Asc,
                },
            ],
        )
    }

    fn modal(profiles: Vec<SortProfile>, cursor: usize, target: SortApplyTarget) -> Modal {
        Modal::SortProfile {
            profiles,
            cursor,
            editing: None,
            target,
        }
    }

    fn state_with(m: Modal) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(m);
        state
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
    fn profiles_show_rule_chain_with_directions() {
        let state = state_with(modal(
            vec![chronological_discog()],
            0,
            SortApplyTarget::Queue,
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("chronological_discog"));
        assert!(rendered.contains("album_artist \u{2191}"));
        assert!(rendered.contains("year \u{2191}"));
        assert!(rendered.contains("album \u{2191}"));
        assert!(rendered.contains("track_number \u{2191}"));
    }

    #[test]
    fn descending_direction_shown() {
        let p = profile(
            "release_chronology",
            vec![SortRule {
                field: SortField::Year,
                direction: Direction::Desc,
            }],
        );
        let state = state_with(modal(vec![p], 0, SortApplyTarget::Queue));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("year \u{2193}"));
    }

    #[test]
    fn active_profile_marked() {
        let mut state = state_with(modal(
            vec![profile("by_name", vec![]), profile("by_year", vec![])],
            0,
            SortApplyTarget::Queue,
        ));
        state.queue.sort_profile = Some("by_year".to_string());
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("\u{2022} by_year"));
        assert!(rendered.contains("  by_name"));
        assert!(!rendered.contains("no profile active"));
    }

    #[test]
    fn no_profile_active_note_shown_when_none_applied() {
        let state = state_with(modal(
            vec![profile("by_name", vec![])],
            0,
            SortApplyTarget::Queue,
        ));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("no profile active"));
    }

    #[test]
    fn empty_profile_list_state() {
        let state = state_with(modal(Vec::new(), 0, SortApplyTarget::Queue));
        let rendered = draw_at(90, 20, &state);
        assert!(rendered.contains("no sort profiles"));
        assert!(rendered.contains("press {e} to create one"));
    }

    #[test]
    fn does_not_render_for_other_modal_kinds() {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(Modal::Help {
            context: loxia_core::keymap::InputContext::Normal,
            scroll: 0,
        });
        let rendered = draw_at(90, 20, &state);
        assert!(!rendered.contains("SORT PROFILE"));
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let state = state_with(modal(
            vec![chronological_discog()],
            0,
            SortApplyTarget::Queue,
        ));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn sort_profile_snapshot() {
        let mut state = state_with(modal(
            vec![
                chronological_discog(),
                profile(
                    "release_chronology",
                    vec![
                        SortRule {
                            field: SortField::Year,
                            direction: Direction::Desc,
                        },
                        SortRule {
                            field: SortField::Album,
                            direction: Direction::Asc,
                        },
                        SortRule {
                            field: SortField::TrackNumber,
                            direction: Direction::Asc,
                        },
                    ],
                ),
                profile(
                    "name_only",
                    vec![SortRule {
                        field: SortField::Name,
                        direction: Direction::Asc,
                    }],
                ),
            ],
            1,
            SortApplyTarget::Queue,
        ));
        state.queue.sort_profile = Some("chronological_discog".to_string());
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn sort_profile_snapshot_empty() {
        let state = state_with(modal(Vec::new(), 0, SortApplyTarget::Column));
        let rendered = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }
}
