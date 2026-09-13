//! Audio output device picker modal (`10-05`, `design_overview` §3.5): switches the audio output
//! device mid-playback. Devices are grouped by driver (`loxia_core::model::group_by_driver` —
//! moved there from `loxia-audio` this same task, since it and `device_label` are pure functions
//! of `AudioDevice`'s own fields with no OS-specific behaviour, and `loxia-tui` cannot depend on
//! `loxia-audio` to reach them otherwise; see `docs/12-decisions.md`).

use loxia_core::model::{AudioDevice, group_by_driver};
use loxia_core::state::AppState;
use loxia_core::state::modal::Modal;
use loxia_core::state::nav::LoadState;
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;

const TITLE: &str = "SELECT AUDIO OUTPUT DEVICE";
/// The only backend this project supports (`docs/05-audio-engine.md`'s locked decision) — not
/// read from config, since there is nothing to choose between.
const ENGINE_NAME: &str = "libmpv";
const MODAL_WIDTH: u16 = 74;

/// Driver id -> the wireframe's own display name (`design_overview` §3.5's own literal casing);
/// an id this table doesn't recognise (mpv's own `ao` list is fixed, so unlikely) falls back to
/// capitalizing the raw string rather than showing nothing.
fn driver_display_name(driver: &str) -> String {
    match driver {
        "alsa" => "ALSA".to_string(),
        "pulse" => "PulseAudio".to_string(),
        "pipewire" => "PipeWire".to_string(),
        "wasapi" => "WASAPI".to_string(),
        "coreaudio" => "CoreAudio".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// How many body rows (below the "Active Engine"/divider lines) the current state needs — one
/// line for loading/error/empty, or one header plus one row per device per driver group.
fn body_row_count(devices: &[AudioDevice], load: &LoadState) -> usize {
    match load {
        LoadState::Idle | LoadState::Loading => 1,
        LoadState::Error(_) => 1,
        _ if devices.is_empty() => 1,
        LoadState::Loaded { .. } => group_by_driver(devices)
            .iter()
            .map(|(_, group)| 1 + group.len())
            .sum(),
    }
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let Some(Modal::DevicePicker {
        devices,
        cursor,
        load,
    }) = &state.modal
    else {
        return;
    };
    if area.width < 3 || area.height < 3 {
        return;
    }

    // Active Engine + divider + body + blank + footer, plus the border itself.
    let content_rows = 2 + body_row_count(devices, load) + 2;
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
        .title(TITLE);
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

    y = render_line(
        f,
        inner,
        y,
        end_y,
        format!("Active Engine: {ENGINE_NAME}"),
        style::style(theme, Role::Fg),
    );

    let rule = theme.glyphs().rule;
    let divider: String = std::iter::repeat_n(rule, inner.width as usize).collect();
    y = render_line(f, inner, y, end_y, divider, style::fg(theme, Role::Dim));

    match load {
        LoadState::Idle | LoadState::Loading => {
            render_line(
                f,
                inner,
                y,
                end_y,
                "Loading…".to_string(),
                style::fg(theme, Role::Dim),
            );
        }
        LoadState::Error(message) => {
            render_line(
                f,
                inner,
                y,
                end_y,
                message.clone(),
                style::style(theme, Role::Error),
            );
        }
        LoadState::Loaded { .. } if devices.is_empty() => {
            render_line(
                f,
                inner,
                y,
                end_y,
                "no output devices found".to_string(),
                style::fg(theme, Role::Dim),
            );
        }
        LoadState::Loaded { .. } => {
            let mut flat_index = 0usize;
            for (driver, group) in group_by_driver(devices) {
                if y >= end_y {
                    break;
                }
                y = render_line(
                    f,
                    inner,
                    y,
                    end_y,
                    driver_display_name(&driver),
                    style::fg(theme, Role::Dim),
                );
                for device in &group {
                    if y >= end_y {
                        break;
                    }
                    let is_cursor = flat_index == *cursor;
                    let is_active = device.id == state.config.audio.device_id;
                    let row_area = Rect::new(inner.x, y, inner.width, 1);
                    hits.push(row_area, HitTarget::ModalField(flat_index));
                    render_device_row(f, row_area, device, is_cursor, is_active, theme);
                    y += 1;
                    flat_index += 1;
                }
            }
        }
    }

    if end_y > inner.y {
        let footer_y = end_y - 1;
        if footer_y >= y {
            render_line(
                f,
                inner,
                footer_y,
                end_y,
                "[ Enter ] Switch    [ Esc ] Cancel".to_string(),
                style::fg(theme, Role::Dim),
            );
        }
    }
}

/// Renders one plain (non-interactive) line at `y` and returns `y + 1` — the running cursor every
/// caller above threads through, skipped entirely once `y` reaches `end_y`.
fn render_line(
    f: &mut Frame,
    inner: Rect,
    y: u16,
    end_y: u16,
    text: String,
    style: ratatui::style::Style,
) -> u16 {
    if y < end_y {
        let row_area = Rect::new(inner.x, y, inner.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text, style))),
            row_area,
        );
    }
    y + 1
}

fn render_device_row(
    f: &mut Frame,
    row_area: Rect,
    device: &AudioDevice,
    is_cursor: bool,
    is_active: bool,
    theme: &Theme,
) {
    let cursor_glyph = if is_cursor { '▸' } else { ' ' };
    let active_glyph = if is_active { '•' } else { '○' };
    let row_style = if is_cursor {
        style::selection(theme, true)
    } else {
        style::style(theme, Role::Fg)
    };

    let spans = vec![Span::raw(format!(
        "{cursor_glyph} {active_glyph} {}",
        device.description
    ))];

    f.render_widget(Paragraph::new(Line::from(spans)).style(row_style), row_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn device(id: &str, description: &str, driver: &str) -> AudioDevice {
        AudioDevice {
            id: id.to_string(),
            description: description.to_string(),
            driver: driver.to_string(),
        }
    }

    fn state_with(modal: Modal) -> AppState {
        let mut state = fixtures::fixture_empty();
        state.modal = Some(modal);
        state
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

    fn three_devices() -> Vec<AudioDevice> {
        vec![
            device(
                "alsa/hw:0,0",
                "Direct Hardware Passthrough (hw:0,0)",
                "alsa",
            ),
            device(
                "pipewire/a",
                "Default System Output Sink (Shared)",
                "pipewire",
            ),
            device("pulse/a", "USB Headphones DAC", "pulse"),
        ]
    }

    /// Every row this widget draws is one uniform `HitTarget` across its own whole width, so a
    /// single sample column is enough to enumerate one hit per row without triple-counting a
    /// multi-cell-wide region.
    fn row_hits(hits: &HitMap, sample_col: u16, h: u16) -> Vec<HitTarget> {
        (0..h)
            .filter_map(|row| hits.hit(sample_col, row))
            .copied()
            .collect()
    }

    #[test]
    fn devices_grouped_by_driver() {
        let mut state = state_with(Modal::DevicePicker {
            devices: three_devices(),
            cursor: 0,
            load: LoadState::Loaded { total: 3 },
        });
        state.config.audio.device_id = "alsa/hw:0,0".to_string();
        let (rendered, _) = draw_at(90, 20, &state);

        assert!(rendered.contains("ALSA"));
        assert!(rendered.contains("PipeWire"));
        assert!(rendered.contains("PulseAudio"));
        let alsa_pos = rendered.find("ALSA").unwrap();
        let pipewire_pos = rendered.find("PipeWire").unwrap();
        let pulse_pos = rendered.find("PulseAudio").unwrap();
        assert!(
            alsa_pos < pipewire_pos && pipewire_pos < pulse_pos,
            "groups must render in `group_by_driver`'s own most-capable-first order"
        );
    }

    #[test]
    fn driver_headers_are_not_selectable() {
        let state = state_with(Modal::DevicePicker {
            devices: three_devices(),
            cursor: 0,
            load: LoadState::Loaded { total: 3 },
        });
        let (_, hits) = draw_at(90, 20, &state);

        // Exactly one `ModalField` hit per device — never one for a driver header row.
        let field_hits = row_hits(&hits, 10, 20)
            .into_iter()
            .filter(|t| matches!(t, HitTarget::ModalField(_)))
            .count();
        assert_eq!(field_hits, 3);
    }

    #[test]
    fn active_device_marked() {
        let mut state = state_with(Modal::DevicePicker {
            devices: three_devices(),
            cursor: 0,
            load: LoadState::Loaded { total: 3 },
        });
        state.config.audio.device_id = "pulse/a".to_string();
        let (rendered, _) = draw_at(90, 20, &state);

        // The active row's own bullet is filled; every other row's is hollow.
        assert!(rendered.contains("• USB Headphones DAC"));
        assert!(rendered.contains("○ Direct Hardware Passthrough"));
        assert!(rendered.contains("○ Default System Output Sink"));
    }

    #[test]
    fn loading_state_rendered() {
        let state = state_with(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loading,
        });
        let (rendered, hits) = draw_at(90, 20, &state);
        assert!(rendered.contains("Loading"));
        assert_eq!(
            row_hits(&hits, 10, 20)
                .into_iter()
                .filter(|t| matches!(t, HitTarget::ModalField(_)))
                .count(),
            0
        );
    }

    #[test]
    fn empty_list_state() {
        let state = state_with(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loaded { total: 0 },
        });
        let (rendered, _) = draw_at(90, 20, &state);
        assert!(rendered.contains("no output devices found"));
    }

    #[test]
    fn error_state_rendered() {
        let state = state_with(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Error("enumeration failed".to_string()),
        });
        let (rendered, _) = draw_at(90, 20, &state);
        assert!(rendered.contains("enumeration failed"));
    }

    #[test]
    fn footer_keys_are_shown() {
        let state = state_with(Modal::DevicePicker {
            devices: three_devices(),
            cursor: 0,
            load: LoadState::Loaded { total: 3 },
        });
        let (rendered, _) = draw_at(90, 20, &state);
        assert!(rendered.contains("Enter"));
        assert!(rendered.contains("Switch"));
        assert!(rendered.contains("Esc"));
        assert!(rendered.contains("Cancel"));
    }

    #[test]
    fn does_not_render_for_other_modal_kinds() {
        let state = state_with(Modal::Help {
            context: loxia_core::keymap::InputContext::Normal,
            scroll: 0,
        });
        let (rendered, hits) = draw_at(90, 20, &state);
        assert!(!rendered.contains(TITLE));
        assert_eq!(row_hits(&hits, 10, 20).len(), 0);
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let state = state_with(Modal::DevicePicker {
            devices: three_devices(),
            cursor: 0,
            load: LoadState::Loaded { total: 3 },
        });
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn device_picker_snapshot() {
        let mut state = state_with(Modal::DevicePicker {
            devices: three_devices(),
            cursor: 1,
            load: LoadState::Loaded { total: 3 },
        });
        state.config.audio.device_id = "alsa/hw:0,0".to_string();
        let (rendered, _) = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn device_picker_snapshot_loading() {
        let state = state_with(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loading,
        });
        let (rendered, _) = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn device_picker_snapshot_empty() {
        let state = state_with(Modal::DevicePicker {
            devices: Vec::new(),
            cursor: 0,
            load: LoadState::Loaded { total: 0 },
        });
        let (rendered, _) = draw_at(90, 20, &state);
        insta::assert_snapshot!(rendered);
    }
}
