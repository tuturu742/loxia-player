//! Sub-cell-precision progress bar (`docs/07-ui-spec.md` §7). Renders only the bar itself — the
//! surrounding `mm:ss` timestamps are `widgets/player_bar.rs`'s job, which is also why
//! `HitTarget::SeekBar` is registered here rather than over the whole line.

use std::time::Duration;

use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::hit::{HitMap, HitTarget};
use crate::style;

/// Eighth-cell partial blocks, `[1/8 .. 7/8]` — index `n` is `(n+1)/8` filled.
const PARTIAL_UNICODE: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
const FULL_UNICODE: char = '█';
const EMPTY_UNICODE: char = '░';
/// Distinct from both the played (`█`) and unplayed (`░`) glyphs, so the buffered-but-unplayed
/// region is visually its own thing.
const BUFFERED_UNICODE: char = '▒';

const FULL_ASCII: char = '#';
const EMPTY_ASCII: char = '-';
const BUFFERED_ASCII: char = '=';

pub fn render_progress(
    f: &mut Frame,
    area: Rect,
    pos: Duration,
    dur: Duration,
    buffered: Option<f32>,
    theme: &Theme,
    hits: &mut HitMap,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = area.width as usize;

    // Never divide by zero: an unknown/zero duration renders a fully empty bar.
    let fraction = if dur.is_zero() {
        0.0
    } else {
        (pos.as_secs_f64() / dur.as_secs_f64()).clamp(0.0, 1.0)
    };

    let bar = build_bar(width, fraction, buffered, theme.ascii_only);
    let row = Rect::new(area.x, area.y, area.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            bar,
            style::fg(theme, Role::ProgressFilled),
        ))),
        row,
    );

    if !dur.is_zero() {
        hits.push(row, HitTarget::SeekBar);
    }
}

fn build_bar(width: usize, fraction: f64, buffered: Option<f32>, ascii_only: bool) -> String {
    if width == 0 {
        return String::new();
    }

    if ascii_only {
        let full_cells = (width as f64 * fraction).round() as usize;
        let buffered_end = buffered
            .map(|b| (width as f64 * f64::from(b).clamp(0.0, 1.0)).round() as usize)
            .unwrap_or(full_cells)
            .max(full_cells);
        return (0..width)
            .map(|i| {
                if i < full_cells {
                    FULL_ASCII
                } else if i < buffered_end {
                    BUFFERED_ASCII
                } else {
                    EMPTY_ASCII
                }
            })
            .collect();
    }

    let total_eighths = (width as f64 * fraction * 8.0).round() as usize;
    let full_cells = (total_eighths / 8).min(width);
    let remainder = total_eighths % 8;
    let buffered_end = buffered
        .map(|b| (width as f64 * f64::from(b).clamp(0.0, 1.0)).round() as usize)
        .unwrap_or(full_cells)
        .max(full_cells);

    let mut out = String::with_capacity(width);
    for i in 0..width {
        if i < full_cells {
            out.push(FULL_UNICODE);
        } else if i == full_cells && remainder > 0 {
            out.push(PARTIAL_UNICODE[remainder - 1]);
        } else if i < buffered_end {
            out.push(BUFFERED_UNICODE);
        } else {
            out.push(EMPTY_UNICODE);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::theme::Theme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, pos_secs: u64, dur_secs: u64, buffered: Option<f32>) -> (String, HitMap) {
        render_at_ascii(w, pos_secs, dur_secs, buffered, false)
    }

    fn render_at_ascii(
        w: u16,
        pos_secs: u64,
        dur_secs: u64,
        buffered: Option<f32>,
        ascii_only: bool,
    ) -> (String, HitMap) {
        let backend = TestBackend::new(w, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme {
            ascii_only,
            ..Theme::default()
        };
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                let area = f.area();
                render_progress(
                    f,
                    area,
                    Duration::from_secs(pos_secs),
                    Duration::from_secs(dur_secs),
                    buffered,
                    &theme,
                    &mut hits,
                )
            })
            .unwrap();
        (format!("{:?}", terminal.backend().buffer()), hits)
    }

    #[test]
    fn progress_subcell_precision() {
        let (half, _) = render_at(10, 50, 100, None);
        assert!(half.contains(&FULL_UNICODE.to_string().repeat(5)));
        assert!(!half.contains(PARTIAL_UNICODE[0]) && !half.contains(PARTIAL_UNICODE[3]));

        let (fifty_five, _) = render_at(10, 55, 100, None);
        assert!(fifty_five.contains(&FULL_UNICODE.to_string().repeat(5)));
        assert!(PARTIAL_UNICODE.iter().any(|c| fifty_five.contains(*c)));
    }

    #[test]
    fn progress_zero_duration_does_not_panic() {
        let (rendered, _) = render_at(10, 0, 0, None);
        assert!(rendered.contains(&EMPTY_UNICODE.to_string().repeat(10)));
    }

    #[test]
    fn progress_registers_seekbar_hit_target() {
        let (_, hits) = render_at(10, 50, 100, None);
        assert_eq!(hits.hit(5, 0), Some(&HitTarget::SeekBar));
    }

    #[test]
    fn buffered_region_uses_distinct_glyph() {
        let (rendered, _) = render_at(10, 20, 100, Some(0.8));
        assert!(rendered.contains(BUFFERED_UNICODE));
        assert!(rendered.contains(EMPTY_UNICODE));
        assert!(rendered.contains(FULL_UNICODE));
    }

    #[test]
    fn ascii_only_progress_has_no_multibyte_chars() {
        let (rendered, _) = render_at_ascii(10, 55, 100, Some(0.8), true);
        for line in rendered.lines() {
            if let Some(content) = line
                .trim()
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
            {
                for c in content.chars() {
                    assert_eq!(c.len_utf8(), 1, "{c:?} is not a single ASCII byte");
                }
            }
        }
    }
}
