//! Header bar: app title, offline indicator, and download status
//! (`docs/07-ui-spec.md`, task `04-05`).
//!
//! Renders a single line: `<title>  ·  <clock>` on the left, and on the right an offline
//! indicator (when disconnected) followed by an active-download counter (when any downloads
//! are queued or in flight). Never renders a token or stream URL — only a bare count and a
//! percentage.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::style::Theme;

/// Download progress summary for the header's right-hand status: how many downloads are
/// still active (queued or in-flight) and the average progress (0-100) across them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadStatus {
    pub active: usize,
    pub percent: u8,
}

impl DownloadStatus {
    pub const fn none() -> Self {
        Self {
            active: 0,
            percent: 0,
        }
    }
}

/// Everything the header needs to render a single frame. Deliberately plain data rather than a
/// borrowed `&AppState` — the header does not know about the reducer, only about the handful of
/// fields it displays.
#[derive(Debug, Clone)]
pub struct HeaderState<'a> {
    pub title: &'a str,
    pub clock: (u8, u8),
    pub offline: bool,
    pub downloads: DownloadStatus,
}

pub struct Header<'a> {
    state: HeaderState<'a>,
    theme: &'a Theme,
}

impl<'a> Header<'a> {
    pub fn new(state: HeaderState<'a>, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    fn left_text(&self) -> String {
        format!(
            "{}  ·  {:02}:{:02}",
            self.state.title, self.state.clock.0, self.state.clock.1
        )
    }

    fn right_text(&self) -> String {
        let mut parts = Vec::new();
        if self.state.offline {
            parts.push("⚠ offline".to_string());
        }
        if self.state.downloads.active > 0 {
            parts.push(format!(
                "⇣ {} ({}%)",
                self.state.downloads.active, self.state.downloads.percent
            ));
        }
        parts.join("  ")
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let row = area.y;
        let bg = Style::default().bg(self.theme.header_bg).fg(self.theme.fg);
        for x in area.x..area.x + area.width {
            buf[(x, row)].set_style(bg);
        }

        let left = self.left_text();
        buf.set_string(area.x, row, &left, Style::default().fg(self.theme.fg));

        let right = self.right_text();
        if right.is_empty() {
            return;
        }
        let right_width = unicode_width::UnicodeWidthStr::width(right.as_str()) as u16;
        let right_x = if right_width < area.width {
            area.x + area.width - right_width
        } else {
            area.x
        };
        let right_style = if self.state.offline {
            Style::default().fg(self.theme.warning)
        } else {
            Style::default().fg(self.theme.accent)
        };
        buf.set_string(right_x, row, &right, right_style);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::style::Theme;

    fn render_to_string(width: u16, height: u16, state: HeaderState<'_>, theme: &Theme) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| {
                let header = Header::new(state, theme);
                frame.render_widget(header, frame.area());
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn header_snapshot_offline_with_downloads() {
        let theme = Theme::default();
        let state = HeaderState {
            title: "loxia",
            clock: (14, 32),
            offline: true,
            downloads: DownloadStatus {
                active: 3,
                percent: 42,
            },
        };
        let rendered = render_to_string(60, 1, state, &theme);
        insta::assert_snapshot!(rendered);
    }
}
