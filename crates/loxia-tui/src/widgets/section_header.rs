//! Non-selectable section header rows: `── ALBUMS (2) ──────` (`docs/07-ui-spec.md` §5). Never
//! given the cursor style and never registered as a hit target — a click on one must do nothing,
//! and the reducer's own `selectable_indices()` already skips them for keyboard navigation.

use loxia_core::model::SectionHeader;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::style;
use crate::text;

pub fn render(f: &mut Frame, area: Rect, header: &SectionHeader, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rule = theme.glyphs().rule;
    let lead: String = std::iter::repeat_n(rule, 2).collect();
    let mut line = format!("{lead} {} ({}) ", header.label, header.count);

    let width = area.width as usize;
    let current_w = text::width(&line);
    if current_w < width {
        line.push_str(&std::iter::repeat_n(rule, width - current_w).collect::<String>());
    } else {
        line = text::truncate(&line, width).into_owned();
    }

    let row_area = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(line, style::fg(theme, Role::Dim)))),
        row_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::model::SectionKind;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_label_and_count() {
        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let header = SectionHeader {
            label: "APPEARS ON".to_string(),
            count: 2,
            kind: SectionKind::AppearsOn,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, &header, &theme)
            })
            .unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("APPEARS ON (2)"));
    }
}
