//! Theme -> ratatui::Style resolution — the only place a `ThemeColor` becomes a
//! `ratatui::style::Color` (`docs/07-ui-spec.md` §12); every widget calls through here rather
//! than resolving a `Role` itself.

use loxia_core::theme::{Role, Theme, ThemeColor};
use ratatui::style::{Color, Style};

/// Exposed `pub(crate)` for widgets that need to build a `Style` mixing roles (e.g. the sidebar's
/// hint digit, which keeps its row's background but overrides just the foreground to `Dim`) —
/// everything else should reach for `style`/`fg`/`selection`/`border` instead of calling this
/// directly.
pub(crate) fn to_color(color: ThemeColor) -> Color {
    match color {
        ThemeColor::Reset => Color::Reset,
        ThemeColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
        ThemeColor::Indexed(i) => Color::Indexed(i),
    }
}

/// The full style for content painted in `role`'s colour, sitting on the theme's normal
/// background — the general-purpose helper most widgets reach for (e.g. `style(theme,
/// Role::Error)` for an error line).
pub fn style(theme: &Theme, role: Role) -> Style {
    Style::default()
        .bg(to_color(theme.color(Role::Bg)))
        .fg(to_color(theme.color(role)))
}

/// Foreground only, no background component — for an inline span within an already-styled line
/// (an icon, a status glyph) where re-asserting the background would needlessly override
/// whatever it already is.
pub fn fg(theme: &Theme, role: Role) -> Style {
    Style::default().fg(to_color(theme.color(role)))
}

/// The surface a modal paints itself on — the theme's own background, under everything the modal
/// draws.
///
/// `Clear` resets a modal's area to the terminal's *default* colours, and a `Block` paints only its
/// border. So without this, only the cells that happen to carry a styled span come out themed and
/// every gap between them shows the terminal background instead — reported on the help sheet, which
/// is mostly gaps (`docs/12-decisions.md`). Every modal block must carry it.
pub fn modal_surface(theme: &Theme) -> Style {
    style(theme, Role::Fg)
}

/// The selected-row highlight. Focused uses the theme's dedicated `selection_bg`/`selection_fg`
/// pair at full strength; unfocused uses `border` as a muted background with the normal `fg` —
/// still visibly marked as selected, but clearly subordinate to whichever column is actually
/// focused. `docs/07-ui-spec.md` §12 names the focused pair but not an unfocused treatment; this
/// is `04-01`'s own reasonable choice.
pub fn selection(theme: &Theme, focused: bool) -> Style {
    if focused {
        Style::default()
            .bg(to_color(theme.color(Role::SelectionBg)))
            .fg(to_color(theme.color(Role::SelectionFg)))
    } else {
        Style::default()
            .bg(to_color(theme.color(Role::Border)))
            .fg(to_color(theme.color(Role::Fg)))
    }
}

/// A border's colour: `border_focus` for the focused pane, `border` otherwise — the pairing
/// `Role::Border`/`Role::BorderFocus` exist for.
pub fn border(theme: &Theme, focused: bool) -> Style {
    let role = if focused {
        Role::BorderFocus
    } else {
        Role::Border
    };
    Style::default().fg(to_color(theme.color(role)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::theme::BUILTIN_THEME_NAMES;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::widgets::{Block, Borders, Gauge, List, ListItem};

    fn render_sample(theme: &Theme) -> String {
        let backend = TestBackend::new(28, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(border(theme, true))
                    .style(style(theme, Role::Fg));
                let items = vec![
                    ListItem::new("Alpha").style(style(theme, Role::Fg)),
                    ListItem::new("Beta").style(selection(theme, true)),
                    ListItem::new("Gamma").style(style(theme, Role::Fg)),
                ];
                let list = List::new(items).block(block);
                f.render_widget(list, Rect::new(0, 0, area.width, 5));

                let gauge = Gauge::default()
                    .gauge_style(style(theme, Role::ProgressFilled))
                    .ratio(0.6)
                    .label("");
                f.render_widget(gauge, Rect::new(0, 5, area.width, 1));
            })
            .unwrap();

        format!("{:?}", terminal.backend().buffer())
    }

    #[test]
    fn theme_snapshot_per_theme() {
        for name in BUILTIN_THEME_NAMES {
            let theme = Theme::builtin(name).unwrap();
            let rendered = render_sample(&theme);
            insta::with_settings!({ snapshot_suffix => *name }, {
                insta::assert_snapshot!(rendered);
            });
        }
    }
}
