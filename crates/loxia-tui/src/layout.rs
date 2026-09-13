//! Root zone computation and responsive degradation (`docs/07-ui-spec.md` §§1-2).

use loxia_core::state::AppState;
use ratatui::layout::{Constraint, Layout, Rect};

pub struct Zones {
    pub header: Rect,
    pub sidebar: Option<Rect>,
    pub canvas: Rect,
    pub player: Rect,
}

/// Vertical split: header (1 row), body (everything else), player bar (3 rows). The body then
/// splits horizontally into a fixed 16-column sidebar and the canvas — except in Zen mode
/// (`state.zen_mode`), which gives the whole body to the canvas and returns `sidebar: None`.
pub fn zones(area: Rect, state: &AppState) -> Zones {
    // 4 rows: the player bar's own top border plus its three content lines
    // (track / seek+transport / technical readout).
    let [header, body, player] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(4),
    ])
    .areas(area);

    if state.zen_mode {
        // Zen draws its own footer at the bottom of its canvas, and `render::draw` skips the global
        // player bar entirely — so the canvas must span the player zone too. Leaving it carved out
        // left three dead rows below Zen's footer, which read as the status bar "moving up"
        // (`docs/12-decisions.md`).
        let canvas = Rect::new(body.x, body.y, body.width, body.height + player.height);
        return Zones {
            header,
            sidebar: None,
            canvas,
            player,
        };
    }

    let [sidebar, canvas] =
        Layout::horizontal([Constraint::Length(16), Constraint::Min(0)]).areas(body);

    Zones {
        header,
        sidebar: Some(sidebar),
        canvas,
        player,
    }
}

pub struct CanvasPlan {
    pub columns: Vec<Rect>,
    pub inspector: Option<Rect>,
}

/// "Full" width isn't given a literal cell count anywhere in `docs/07-ui-spec.md` §2 (only "24
/// cells" for the narrow case is) — 40 is this task's own reasonable choice, wide enough for a
/// full metadata dump (codec, sample rate, bit depth, ReplayGain, etc.) as opposed to the
/// abbreviated 24-cell panel. See `docs/12-decisions.md`.
const INSPECTOR_FULL_WIDTH: u16 = 40;
const INSPECTOR_NARROW_WIDTH: u16 = 24;

enum InspectorMode {
    Hidden,
    Narrow,
    Full,
}

/// The degradation ladder (`docs/07-ui-spec.md` §2), keyed on the canvas width passed to
/// [`plan_canvas`]:
///
/// | Width   | Data columns | Inspector |
/// | ------- | ------------ | --------- |
/// | >= 160  | 3            | full      |
/// | 120-159 | 3            | 24 cells  |
/// | 100-119 | 2            | full      |
/// | 80-99   | 1            | hidden    |
fn degradation(width: u16) -> (usize, InspectorMode) {
    if width >= 160 {
        (3, InspectorMode::Full)
    } else if width >= 120 {
        (3, InspectorMode::Narrow)
    } else if width >= 100 {
        (2, InspectorMode::Full)
    } else {
        (1, InspectorMode::Hidden)
    }
}

/// Splits `canvas` into up to `column_count` equal-width Miller columns plus an inspector panel,
/// per the degradation ladder for `canvas.width`. `column_count` is clamped down to whatever the
/// ladder allows at this width — the caller (the Miller view) does not need to know the ladder
/// itself, only how many columns it actually has to show.
pub fn plan_canvas(canvas: Rect, column_count: usize) -> CanvasPlan {
    let (max_columns, inspector_mode) = degradation(canvas.width);
    let visible_columns = column_count.min(max_columns);

    let inspector_width = match inspector_mode {
        InspectorMode::Hidden => None,
        InspectorMode::Narrow => Some(INSPECTOR_NARROW_WIDTH),
        InspectorMode::Full => Some(INSPECTOR_FULL_WIDTH),
    };

    let (columns_area, inspector) = match inspector_width {
        Some(w) if w < canvas.width => {
            let [cols, insp] =
                Layout::horizontal([Constraint::Min(0), Constraint::Length(w)]).areas(canvas);
            (cols, Some(insp))
        }
        _ => (canvas, None),
    };

    let columns = if visible_columns == 0 {
        Vec::new()
    } else {
        let constraints = vec![Constraint::Ratio(1, visible_columns as u32); visible_columns];
        Layout::horizontal(constraints).split(columns_area).to_vec()
    };

    CanvasPlan { columns, inspector }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::state::AppState;
    use proptest::prelude::*;

    fn area_tiles_exactly(outer: Rect, parts: &[Rect]) {
        let total_area: u64 = parts
            .iter()
            .map(|r| u64::from(r.width) * u64::from(r.height))
            .sum();
        assert_eq!(
            total_area,
            u64::from(outer.width) * u64::from(outer.height),
            "parts must exactly tile the outer area with no gap or overlap"
        );
    }

    proptest! {
        #[test]
        fn zones_sum_to_area(w in 80u16..=400, h in 24u16..=120) {
            let area = Rect::new(0, 0, w, h);
            let state = AppState::default();
            let z = zones(area, &state);
            let mut parts = vec![z.header, z.canvas, z.player];
            if let Some(sidebar) = z.sidebar {
                parts.push(sidebar);
            }
            area_tiles_exactly(area, &parts);
        }

        #[test]
        fn too_small_does_not_panic(w in 1u16..80, h in 1u16..24) {
            let area = Rect::new(0, 0, w, h);
            let state = AppState::default();
            // Must not panic; zones() itself has no too-small guard (draw()'s job), so this only
            // asserts the layout math survives, not that the result is meaningful.
            let _ = zones(area, &state);
        }
    }

    #[test]
    fn zen_hides_sidebar() {
        let area = Rect::new(0, 0, 120, 40);
        let state = AppState {
            zen_mode: true,
            ..AppState::default()
        };
        let z = zones(area, &state);
        assert!(z.sidebar.is_none());
        assert_eq!(z.canvas.width, area.width);
    }

    #[test]
    fn degradation_ladder() {
        let cases: &[(u16, usize, bool, Option<u16>)] = &[
            (80, 1, false, None),
            (99, 1, false, None),
            (100, 2, true, Some(INSPECTOR_FULL_WIDTH)),
            (119, 2, true, Some(INSPECTOR_FULL_WIDTH)),
            (120, 3, true, Some(INSPECTOR_NARROW_WIDTH)),
            (159, 3, true, Some(INSPECTOR_NARROW_WIDTH)),
            (160, 3, true, Some(INSPECTOR_FULL_WIDTH)),
            (240, 3, true, Some(INSPECTOR_FULL_WIDTH)),
        ];
        for &(width, expected_cols, expects_inspector, expected_width) in cases {
            let canvas = Rect::new(0, 0, width, 30);
            let plan = plan_canvas(canvas, 3);
            assert_eq!(
                plan.columns.len(),
                expected_cols,
                "width {width}: wrong column count"
            );
            assert_eq!(
                plan.inspector.is_some(),
                expects_inspector,
                "width {width}: wrong inspector presence"
            );
            if let (Some(inspector), Some(expected_w)) = (plan.inspector, expected_width) {
                assert_eq!(
                    inspector.width, expected_w,
                    "width {width}: wrong inspector width"
                );
            }
        }
    }
}
