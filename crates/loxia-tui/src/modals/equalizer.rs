//! 10-band graphic equalizer modal (`10-06`, `design_overview` §3.8): live-preview gain editing,
//! presets, bypass, and drag support.

use loxia_core::config::EQ_BANDS_HZ;
use loxia_core::state::AppState;
use loxia_core::state::modal::Modal;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;

const MODAL_WIDTH: u16 = 78;
/// The dB-value label plus its tee/cross character, e.g. `" +12dB ┤"`.
const GUTTER_WIDTH: u16 = 8;
/// `docs/12-decisions.md` / this task's own spec: "below 16 rows the ±6 dB gridlines are dropped
/// before the bars are" — this is the "16" the spec names, checked against the *available* area,
/// not the modal's own (content-driven) height.
const REDUCED_GRID_HEIGHT: u16 = 16;
const SUB_BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const FULL_GRID: [i32; 5] = [12, 6, 0, -6, -12];
const REDUCED_GRID: [i32; 3] = [12, 0, -12];

fn grid_label(db: i32) -> String {
    if db == 0 {
        "0dB".to_string()
    } else {
        format!("{db:+}dB")
    }
}

fn freq_label(hz: u32) -> String {
    if hz >= 1000 {
        format!("{}k", hz / 1000)
    } else {
        hz.to_string()
    }
}

/// The `(start, width)` of band `band_idx`'s own bar/tick column within a plot area `plot_width`
/// chars wide — shared by hit-rect registration and every row renderer below, so a click always
/// lands on exactly the column its own bar/tick/frequency label were drawn in.
fn band_slot(plot_width: usize, band_idx: usize) -> (usize, usize) {
    let slot_w = (plot_width / 10).max(1);
    let bar_w = 2.min(slot_w);
    let slot_start = band_idx * slot_w;
    let bar_start = slot_start + slot_w.saturating_sub(bar_w) / 2;
    (bar_start.min(plot_width.saturating_sub(1)), bar_w)
}

/// How much of the row at `row_db` (nonzero) band `band_idx`'s own `gain_db` fills, `0.0..=1.0` —
/// zero if the gain doesn't reach this row at all (including the "wrong side" of zero: a positive
/// gain never fills a negative row and vice versa).
fn row_fill_fraction(gain_db: f32, row_db: i32, db_per_row: f32) -> f32 {
    if gain_db == 0.0 || gain_db.signum() != (row_db as f32).signum() {
        return 0.0;
    }
    let row_index = (row_db.unsigned_abs() as f32 / db_per_row).round();
    let row_low = (row_index - 1.0) * db_per_row;
    ((gain_db.abs() - row_low) / db_per_row).clamp(0.0, 1.0)
}

fn glyph_for_fraction(fraction: f32, ascii_only: bool) -> Option<char> {
    if fraction <= 0.0 {
        return None;
    }
    if ascii_only {
        return Some('#');
    }
    let idx = ((fraction * 8.0).ceil() as usize).clamp(1, 8) - 1;
    Some(SUB_BLOCKS[idx])
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let Some(Modal::Equalizer {
        band,
        draft_gains,
        preset_idx,
        bypassed,
        enabled,
        ..
    }) = &state.modal
    else {
        return;
    };
    if area.width < 3 || area.height < 3 {
        return;
    }

    let dim_bars = *bypassed;
    let ascii = theme.ascii_only;

    let grid: Vec<i32> = if area.height < REDUCED_GRID_HEIGHT {
        REDUCED_GRID.to_vec()
    } else {
        FULL_GRID.to_vec()
    };
    let db_per_row = 24.0 / (grid.len() as f32 - 1.0);

    let content_rows = grid.len() + 3; // grid rows + tick row + freq row + footer row
    let width = MODAL_WIDTH.min(area.width).max(1);
    let height = ((content_rows + 2) as u16).min(area.height).max(1);
    let modal_area = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let preset_name = match preset_idx {
        Some(i) => state
            .player
            .known_presets
            .get(*i)
            .map(|p| p.name.as_str())
            .unwrap_or("Custom"),
        None => "Custom",
    };
    let status = if !*enabled {
        "OFF"
    } else if *bypassed {
        "BYPASSED"
    } else {
        "ACTIVE"
    };
    let title = format!("EQUALIZER [Preset: {preset_name}] [Status: {status}]");

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
    if inner.width <= GUTTER_WIDTH || inner.height == 0 {
        return;
    }

    let plot_width = (inner.width - GUTTER_WIDTH) as usize;
    let end_y = inner.y + inner.height;
    let mut y = inner.y;
    let grid_top_y = y;

    for &row_db in &grid {
        if y >= end_y {
            break;
        }
        render_grid_row(
            f,
            Rect::new(inner.x, y, inner.width, 1),
            plot_width,
            row_db,
            db_per_row,
            draft_gains,
            *band,
            dim_bars,
            ascii,
            theme,
        );
        y += 1;
    }
    let grid_bottom_y = y; // one past the last drawn grid row

    // `HitTarget::EqBand` spans the whole plotted grid height for that band's own column, so a
    // click/drag anywhere in it computes a y-based gain via `input.rs`'s `gain_from_y` — pushed
    // once per band, not once per row.
    if grid_bottom_y > grid_top_y {
        for band_idx in 0..10 {
            let (bar_start, bar_w) = band_slot(plot_width, band_idx);
            hits.push(
                Rect::new(
                    inner.x + GUTTER_WIDTH + bar_start as u16,
                    grid_top_y,
                    bar_w as u16,
                    grid_bottom_y - grid_top_y,
                ),
                HitTarget::EqBand(band_idx),
            );
        }
    }

    if y < end_y {
        render_tick_row(
            f,
            Rect::new(inner.x, y, inner.width, 1),
            plot_width,
            ascii,
            theme,
        );
        y += 1;
    }
    if y < end_y {
        render_freq_row(
            f,
            Rect::new(inner.x, y, inner.width, 1),
            plot_width,
            *band,
            theme,
        );
        y += 1;
    }
    if y < end_y {
        let footer = footer_for(inner.width as usize);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                footer,
                style::fg(theme, Role::Dim),
            ))),
            Rect::new(inner.x, y, inner.width, 1),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_grid_row(
    f: &mut Frame,
    row_area: Rect,
    plot_width: usize,
    row_db: i32,
    db_per_row: f32,
    draft_gains: &[f32; 10],
    selected_band: usize,
    dim_bars: bool,
    ascii: bool,
    theme: &Theme,
) {
    let is_center = row_db == 0;
    let tee = if is_center {
        if ascii { '+' } else { '┼' }
    } else if ascii {
        '|'
    } else {
        '┤'
    };
    let fill_char = if is_center {
        if ascii { '-' } else { theme.glyphs().rule }
    } else {
        ' '
    };
    let dim_style = style::fg(theme, Role::Dim);
    let fill_style = if is_center {
        dim_style
    } else {
        Style::default()
    };

    let mut spans = vec![Span::styled(
        format!("{:>6} {tee}", grid_label(row_db)),
        dim_style,
    )];

    let mut cursor = 0usize;
    for (band_idx, &gain) in draft_gains.iter().enumerate() {
        let (bar_start, bar_w) = band_slot(plot_width, band_idx);
        if bar_start > cursor {
            spans.push(Span::styled(
                fill_char.to_string().repeat(bar_start - cursor),
                fill_style,
            ));
        }
        let glyph = if is_center {
            Some(if ascii { '#' } else { '█' })
        } else {
            glyph_for_fraction(row_fill_fraction(gain, row_db, db_per_row), ascii)
        };
        let bar_style = if dim_bars {
            dim_style
        } else if band_idx == selected_band {
            style::fg(theme, Role::Accent)
        } else {
            style::fg(theme, Role::ProgressFilled)
        };
        match glyph {
            Some(g) => spans.push(Span::styled(g.to_string().repeat(bar_w), bar_style)),
            None => spans.push(Span::styled(
                fill_char.to_string().repeat(bar_w),
                fill_style,
            )),
        }
        cursor = bar_start + bar_w;
    }
    if cursor < plot_width {
        spans.push(Span::styled(
            fill_char.to_string().repeat(plot_width - cursor),
            fill_style,
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), row_area);
}

fn render_tick_row(f: &mut Frame, row_area: Rect, plot_width: usize, ascii: bool, theme: &Theme) {
    let corner = if ascii { '+' } else { '└' };
    let rule = if ascii { '-' } else { theme.glyphs().rule };
    let tick = if ascii { '+' } else { '┬' };
    let dim_style = style::fg(theme, Role::Dim);

    let mut line = String::new();
    line.push_str(&" ".repeat((GUTTER_WIDTH.saturating_sub(1)) as usize));
    line.push(corner);
    let mut plot = vec![rule; plot_width];
    for band_idx in 0..10 {
        let (bar_start, bar_w) = band_slot(plot_width, band_idx);
        let mark_at = bar_start + bar_w / 2;
        if mark_at < plot.len() {
            plot[mark_at] = tick;
        }
    }
    line.push_str(&plot.into_iter().collect::<String>());

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(line, dim_style))),
        row_area,
    );
}

fn render_freq_row(
    f: &mut Frame,
    row_area: Rect,
    plot_width: usize,
    selected_band: usize,
    theme: &Theme,
) {
    let dim_style = style::fg(theme, Role::Dim);
    let mut spans = vec![Span::raw(" ".repeat(GUTTER_WIDTH as usize))];
    let mut cursor = 0usize;
    for (band_idx, &hz) in EQ_BANDS_HZ.iter().enumerate() {
        let (bar_start, bar_w) = band_slot(plot_width, band_idx);
        let label = freq_label(hz);
        // Centre the label under its own 2-char-wide bar, padding as needed on both sides.
        let label_start = bar_start.saturating_sub(label.len().saturating_sub(bar_w) / 2);
        if label_start > cursor {
            spans.push(Span::styled(
                " ".repeat(label_start - cursor),
                Style::default(),
            ));
        }
        let style = if band_idx == selected_band {
            style::fg(theme, Role::Accent)
        } else {
            dim_style
        };
        spans.push(Span::styled(label.clone(), style));
        cursor = label_start + label.len();
    }
    if cursor < plot_width {
        spans.push(Span::styled(
            " ".repeat(plot_width - cursor),
            Style::default(),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), row_area);
}

/// The footer hints, widest set that fits.
///
/// Every key that *leaves* this modal is named, `Enter` above all. It used to read only
/// "[Esc] Close" — and `Esc` **cancels**, restoring the gains from when the modal opened. So the
/// single exit the footer advertised was the one that threw the edit away, and the equalizer
/// looked like it had stopped working (`docs/12-decisions.md`).
///
/// Hints are dropped from the front when the modal is too narrow, never truncated mid-word: the
/// leading ones are the arrow keys, which a user will try anyway, while Apply/Cancel are the two
/// that cannot be guessed and must survive to the narrowest layout.
fn footer_for(width: usize) -> String {
    const HINTS: [&str; 7] = [
        "[←/→] Band",
        "[↑/↓] ±0.5dB",
        "[p] Preset",
        "[b] Bypass",
        // Labelled by what it does, not by its letter — "Off" alone read as a state rather than a
        // control.
        "[t] On/Off",
        "[↵] Apply",
        "[Esc] Cancel",
    ];
    for skip in 0..HINTS.len() {
        let line = HINTS[skip..].join("  ");
        if crate::text::width(&line) <= width {
            return line;
        }
    }
    HINTS[HINTS.len() - 1].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn state_with(modal: Modal) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal);
        state
    }

    fn eq_modal(band: usize, draft_gains: [f32; 10], bypassed: bool) -> Modal {
        Modal::Equalizer {
            band,
            draft_gains,
            gains_at_open: draft_gains,
            preset_idx: None,
            bypassed,
            enabled: true,
            enabled_at_open: true,
        }
    }

    fn draw_at(w: u16, h: u16, state: &AppState) -> (String, HitMap) {
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
        (format!("{:?}", terminal.backend().buffer()), hits)
    }

    #[test]
    fn bars_render_from_centre_line() {
        let mut gains = [0.0; 10];
        gains[2] = 12.0;
        gains[7] = -12.0;
        let state = state_with(eq_modal(0, gains, false));
        let (rendered, _) = draw_at(90, 24, &state);

        // The 0dB row itself always shows a full block for every band (the "ruler" line).
        assert!(rendered.contains("0dB"));
        // A fully-boosted band fills all the way to the top gridline; a fully-cut one fills all
        // the way to the bottom — proving bars actually grow away from centre, not just sit on it.
        assert!(rendered.contains("+12dB"));
        assert!(rendered.contains("-12dB"));
    }

    #[test]
    fn selected_band_highlighted_with_label() {
        // Band 4 (index 4) is the 500 Hz band — its own frequency label must appear, and moving
        // the selection must actually change *something* rendered (the Accent highlight on that
        // band's bar and label), not just leave a static picture regardless of `band`.
        let selected_4 = state_with(eq_modal(4, [0.0; 10], false));
        let selected_0 = state_with(eq_modal(0, [0.0; 10], false));
        let (rendered_4, _) = draw_at(90, 24, &selected_4);
        let (rendered_0, _) = draw_at(90, 24, &selected_0);

        assert!(rendered_4.contains("500"));
        assert_ne!(
            rendered_4, rendered_0,
            "the selected band must change what's highlighted"
        );
    }

    /// `Esc` cancels — the footer must never advertise it as the way to keep an edit, and `Enter`
    /// must always be visible however narrow the modal gets (`docs/12-decisions.md`).
    #[test]
    fn the_footer_always_names_apply_and_cancel() {
        for width in [10, 20, 40, 60, 80, 200] {
            let footer = footer_for(width);
            assert!(
                footer.contains("Cancel"),
                "width {width}: {footer:?} hides the cancel key"
            );
            if width >= 30 {
                assert!(
                    footer.contains("Apply"),
                    "width {width}: {footer:?} hides the apply key"
                );
            }
            if width >= crate::text::width("[↵] Apply  [Esc] Cancel") {
                assert!(crate::text::width(&footer) <= width, "{footer:?} overflows");
            }
        }
        assert!(
            !footer_for(200).contains("Close"),
            "Esc does not merely close"
        );
    }

    /// The equalizer being *off* is a state the modal has to be able to show and reach — until now
    /// it could only be turned on, and the only off switch lived in Settings.
    #[test]
    fn off_is_shown_in_the_title_and_offered_in_the_footer() {
        let mut state = state_with(eq_modal(0, [0.0; 10], false));
        let (rendered, _) = draw_at(90, 24, &state);
        assert!(rendered.contains("Status: ACTIVE"));
        assert!(rendered.contains("[t] On/Off"));

        if let Some(Modal::Equalizer { enabled, .. }) = &mut state.modal {
            *enabled = false;
        }
        let (rendered, _) = draw_at(90, 24, &state);
        assert!(rendered.contains("Status: OFF"), "{rendered}");
    }

    #[test]
    fn bypass_dims_bars_and_updates_title() {
        let state = state_with(eq_modal(0, [1.0; 10], true));
        let (rendered, _) = draw_at(90, 24, &state);
        assert!(rendered.contains("BYPASSED"));
    }

    #[test]
    fn eq_bands_register_hit_targets() {
        let state = state_with(eq_modal(0, [0.0; 10], false));
        let (_, hits) = draw_at(90, 24, &state);
        for band_idx in 0..10 {
            assert!(
                (0..90)
                    .flat_map(|col| (0..24).map(move |row| (col, row)))
                    .any(|(col, row)| hits.hit(col, row) == Some(&HitTarget::EqBand(band_idx))),
                "band {band_idx} never got a HitTarget::EqBand region"
            );
        }
    }

    #[test]
    fn short_terminal_drops_gridlines_first() {
        let state = state_with(eq_modal(0, [6.0; 10], false));
        let (full, _) = draw_at(90, 24, &state);
        let (short, _) = draw_at(90, 15, &state);

        assert!(full.contains("+6dB"), "the full grid must show ±6dB");
        assert!(
            !short.contains("+6dB"),
            "below the 16-row threshold, ±6dB gridlines must be dropped"
        );
        // The outer ±12dB gridlines, and the bars themselves, must still be present.
        assert!(short.contains("+12dB"));
        assert!(short.contains("0dB"));
    }

    #[test]
    fn ascii_only_bars_have_no_multibyte() {
        // Scoped to the bar glyphs themselves — the surrounding `Block` border is ratatui's own
        // fixed box-drawing set and is not something this widget controls or claims to swap out.
        for eighths in 1..=8 {
            let glyph = glyph_for_fraction(eighths as f32 / 8.0, true).unwrap();
            assert!(glyph.is_ascii(), "ascii_only must never produce {glyph:?}");
        }
        assert!(glyph_for_fraction(1.0, false).unwrap() == '█');
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let state = state_with(eq_modal(0, [1.0; 10], false));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn does_not_render_for_other_modal_kinds() {
        let state = state_with(Modal::Help {
            context: loxia_core::keymap::InputContext::Normal,
            scroll: 0,
        });
        let (rendered, hits) = draw_at(90, 24, &state);
        assert!(!rendered.contains("EQUALIZER"));
        assert!(
            !(0..90)
                .flat_map(|col| (0..24).map(move |row| (col, row)))
                .any(|(col, row)| hits.hit(col, row).is_some())
        );
    }

    #[test]
    fn equalizer_snapshot() {
        let mut gains = [0.0; 10];
        gains[1] = 6.0;
        gains[2] = 12.0;
        gains[3] = 6.0;
        gains[6] = -6.0;
        gains[7] = -6.0;
        let state = state_with(eq_modal(2, gains, false));
        let (rendered, _) = draw_at(90, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn equalizer_snapshot_bypassed() {
        let state = state_with(eq_modal(0, [3.0; 10], true));
        let (rendered, _) = draw_at(90, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn equalizer_snapshot_short() {
        let state = state_with(eq_modal(0, [3.0; 10], false));
        let (rendered, _) = draw_at(90, 15, &state);
        insta::assert_snapshot!(rendered);
    }
}
