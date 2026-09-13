//! Settings tab (`11-01`, `docs/07-ui-spec.md` §9): a section list on the left, the focused
//! section's own typed controls on the right. Sections 11-02 through 11-07 give the collection
//! fields (servers, sort profiles, EQ presets, keybindings) their own dedicated editors — this
//! view only ever shows the plain scalar controls `reducer::settings::rows_for_section` builds.

use loxia_core::reducer::settings::{Control, SettingsRow, rows_for_section};
use loxia_core::state::settings::SettingsSection;
use loxia_core::state::{AppState, LibmpvStatus};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::hit::{HitMap, HitTarget};
use crate::style;
use crate::text;

const SECTION_LIST_WIDTH: u16 = 16;

pub fn render(f: &mut Frame, canvas: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    if canvas.width == 0 || canvas.height == 0 {
        return;
    }
    let list_width = SECTION_LIST_WIDTH.min(canvas.width);
    let list_area = Rect::new(canvas.x, canvas.y, list_width, canvas.height);
    let content_area = Rect::new(
        canvas.x + list_width,
        canvas.y,
        canvas.width.saturating_sub(list_width),
        canvas.height,
    );

    render_section_list(f, list_area, state, theme, hits);
    if content_area.width > 0 {
        render_content(f, content_area, state, theme, hits);
    }
}

fn render_section_list(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
) {
    // The section list's border and its selected-row highlight brighten when the list itself is the
    // focused level (`Esc` out of the rows), and dim back down once focus is on the rows or has
    // stepped out to the main tab sidebar — the only on-screen cue for the three focus levels.
    let list_focused = state.settings.section_list_focused;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, list_focused));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // `docs/04-state-and-input.md` §7's "second of the three required conflict surfaces" — a
    // badge right on the Keybindings row in this list, not only inside that section's own content.
    let has_conflicts = !state.keymap.validate().is_empty();

    for (i, &section) in SettingsSection::ALL.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let row_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        hits.push(row_area, HitTarget::SettingsSection(section));

        let focused = section == state.settings.section;
        let mut label = section.label().to_string();
        if section == SettingsSection::Keybindings && has_conflicts {
            label.push_str(" \u{26a0}");
        }
        let style = if focused {
            style::selection(theme, list_focused)
        } else {
            style::style(theme, Role::Fg)
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(&label, inner.width as usize).into_owned(),
                style,
            ))),
            row_area,
        );
    }
}

fn render_content(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, hits: &mut HitMap) {
    let section = state.settings.section;
    let server_editor = (section == SettingsSection::Servers)
        .then_some(state.settings.server_editor.as_ref())
        .flatten();
    let sort_editor = (section == SettingsSection::Sorting)
        .then_some(state.settings.sort_profile_editor.as_ref())
        .flatten();
    let eq_editor = (section == SettingsSection::Equalizer)
        .then_some(state.settings.eq_preset_editor.as_ref())
        .flatten();
    let title = match (server_editor, sort_editor, eq_editor) {
        (Some(e), _, _) if e.editing.is_some() => "Servers — Add/Edit".to_string(),
        (Some(_), _, _) => "Servers — Manage".to_string(),
        (_, Some(_), _) => "Sorting — Manage Profiles".to_string(),
        (_, _, Some(_)) => "Equalizer — Manage Presets".to_string(),
        _ => section.label().to_string(),
    };
    // The content pane's border is focused when the rows are the active level — i.e. neither the
    // section list nor the main tab sidebar has stepped in front of them.
    let rows_focused = !state.settings.section_list_focused && !state.nav.sidebar_focused;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, rows_focused))
        .title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(editor) = server_editor {
        render_server_editor(f, inner, state, editor, theme, hits);
        return;
    }
    if let Some(editor) = sort_editor {
        render_sort_editor(f, inner, state, editor, theme);
        return;
    }
    if let Some(editor) = eq_editor {
        render_eq_editor(f, inner, state, editor, theme);
        return;
    }

    if section == SettingsSection::About {
        render_about(f, inner, state, theme);
        return;
    }

    let mut y = inner.y;
    let end_y = inner.y + inner.height;

    let warnings: Vec<&loxia_core::config::ConfigWarning> = state
        .config_warnings
        .iter()
        .filter(|w| warning_belongs_to_section(&w.field, section))
        .collect();
    for warning in &warnings {
        if y >= end_y {
            return;
        }
        let role = match warning.severity {
            loxia_core::config::Severity::Warning => Role::Warning,
            loxia_core::config::Severity::Info => Role::Dim,
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(&warning.message, inner.width as usize).into_owned(),
                style::fg(theme, role),
            ))),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }
    if !warnings.is_empty() {
        y += 1;
    }

    if section == SettingsSection::Keybindings {
        let conflicts = state.keymap.validate();
        if !conflicts.is_empty() && y < end_y {
            // `11-02`: this message used to end "(11-02, not yet built)" — the editor below now
            // exists, so the badge points at it by name instead of naming the task that built it.
            let message = format!(
                "{} keybinding conflict{} — press Enter on \"edit keybindings\" below to resolve",
                conflicts.len(),
                if conflicts.len() == 1 { "" } else { "s" },
            );
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    text::truncate(&message, inner.width as usize).into_owned(),
                    style::fg(theme, Role::Error),
                ))),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 2;
        }
    }

    let rows = rows_for_section(section, &state.config, &state.player.known_devices);
    for (index, row) in rows.iter().enumerate() {
        if y >= end_y {
            break;
        }
        y = render_row(f, inner, y, end_y, index, row, state, theme, hits);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    f: &mut Frame,
    inner: Rect,
    y: u16,
    end_y: u16,
    index: usize,
    row: &SettingsRow,
    state: &AppState,
    theme: &Theme,
    hits: &mut HitMap,
) -> u16 {
    let focused = index == state.settings.cursor;
    let value_style = if focused {
        style::selection(theme, true)
    } else {
        style::fg(theme, Role::Fg)
    };
    let marker = if focused { "\u{25b6} " } else { "  " };
    // `Ctrl+E` is hardcoded, not routed through the keymap — matching the `Space`/`d`/`Tab`
    // precedent every other modal-local key without a natural `ActionId` already uses (`10-06`).
    let mut suffix = String::new();
    if let Control::Text { secret: true, .. } = row.control
        && focused
    {
        suffix.push_str("   [Ctrl+E] reveal");
    }

    // `11-07` (field fix): while this row is under active edit, show the live-typed buffer (not
    // the still-committed config value — the prior code never did, so typing produced no visible
    // feedback at all) with a real cursor, exactly like the three sub-editors' own fields.
    let (value, cursor) = if focused && let Some(buf) = &state.settings.editing {
        (buf.text.clone(), Some(buf.cursor))
    } else {
        (value_string(&row.control, state), None)
    };
    let prefix = format!("{marker}{}: ", row.label);

    let row_area = Rect::new(inner.x, y, inner.width, 1);
    hits.push(row_area, HitTarget::SettingsRow(index));
    f.render_widget(
        Paragraph::new(Line::from(cursor_aware_spans(
            &prefix,
            &value,
            &suffix,
            cursor,
            value_style,
            inner.width as usize,
        ))),
        row_area,
    );
    let mut next_y = y + 1;

    if next_y < end_y {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(&format!("  {}", row.description), inner.width as usize)
                    .into_owned(),
                style::fg(theme, Role::Dim),
            ))),
            Rect::new(inner.x, next_y, inner.width, 1),
        );
        next_y += 1;
    }

    if let Some((error_index, message)) = &state.settings.error
        && *error_index == index
        && next_y < end_y
    {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(&format!("  {message}"), inner.width as usize).into_owned(),
                style::fg(theme, Role::Error),
            ))),
            Rect::new(inner.x, next_y, inner.width, 1),
        );
        next_y += 1;
    }

    next_y
}

fn value_string(control: &Control, state: &AppState) -> String {
    match control {
        Control::Toggle { get, .. } => {
            if get(&state.config) {
                "on".to_string()
            } else {
                "off".to_string()
            }
        }
        Control::Select { get, .. } => get(&state.config),
        Control::Slider { get, unit, .. } => format!("{:.1} {unit}", get(&state.config)),
        Control::Text { get, secret, .. } => {
            let raw = get(&state.config);
            if *secret && !state.settings.reveal_secret {
                "\u{2022}".repeat(8)
            } else {
                raw
            }
        }
        Control::Number { get, .. } => get(&state.config).to_string(),
        Control::Action { label, .. } => label.clone(),
    }
}

const SECRET_MASK_LEN: usize = 8;

fn masked(value: &str, revealed: bool) -> String {
    if revealed {
        value.to_string()
    } else {
        "\u{2022}".repeat(SECRET_MASK_LEN)
    }
}

/// `11-03`: dispatches to the plain profile list or the add/edit form, whichever `editor.editing`
/// currently names — the same "at most one sub-view active" split `Modal::SortProfile`'s own
/// `editing: Option<RuleEditor>` already uses.
fn render_server_editor(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    editor: &loxia_core::state::settings::ServerEditorState,
    theme: &Theme,
    hits: &mut HitMap,
) {
    if area.height == 0 {
        return;
    }
    match &editor.editing {
        None => render_server_list(f, area, state, editor, theme, hits),
        Some(draft) => render_server_form(f, area, draft, theme, hits),
    }
}

fn render_server_list(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    editor: &loxia_core::state::settings::ServerEditorState,
    theme: &Theme,
    hits: &mut HitMap,
) {
    let end_y = area.y + area.height;
    let mut y = area.y;

    if state.config.servers.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no servers configured — press [a] to add one",
                style::fg(theme, Role::Dim),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    } else {
        for (i, server) in state.config.servers.iter().enumerate() {
            if y >= end_y {
                break;
            }
            let row_area = Rect::new(area.x, y, area.width, 1);
            hits.push(row_area, HitTarget::SettingsRow(i));
            let is_cursor = i == editor.cursor;
            let is_active = server.id == state.config.active_server;
            let marker = if is_active { "\u{2022}" } else { " " };
            let text = format!("{marker} {}  ({})", server.name, server.url);
            let row_style = if is_cursor {
                style::selection(theme, true)
            } else {
                style::style(theme, Role::Fg)
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    text::truncate(&text, area.width as usize).into_owned(),
                    row_style,
                ))),
                row_area,
            );
            y += 1;
        }
    }
    y += 1;

    if let Some(error) = &editor.error
        && y < end_y
    {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(error, area.width as usize).into_owned(),
                style::style(theme, Role::Error),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }

    if y < end_y.saturating_sub(1) {
        y = end_y - 1;
    }
    if y < end_y {
        let delete_flag = if editor.delete_data_on_remove {
            "[x]"
        } else {
            "[ ]"
        };
        let footer = format!(
            "[a] add  [Enter] edit  [x] remove  [D] {delete_flag} delete data on remove  [S] switch  [Esc] close"
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(&footer, area.width as usize).into_owned(),
                style::fg(theme, Role::Dim),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

/// One line per [`loxia_core::reducer::settings::server_draft_fields`] entry, in the same order —
/// `field`'s own index into that list is what `draft.field`/the row's own `HitTarget::SettingsRow`
/// both key on, so the two must never drift apart.
/// Builds `prefix` + `value` + `suffix` as spans, highlighting a single character cell within
/// `value` at `cursor` (a character index) when `Some` — the field currently under active edit,
/// as opposed to a field the row-navigation cursor merely rests on (`cursor: None`, which every
/// non-editing row already passed, rendering as one flat span exactly as before this fix). Found
/// missing in the field alongside cursor *movement* itself: with no visible insertion point,
/// "which field am I editing" and "where will the next keystroke land" were both invisible
/// (`docs/12-decisions.md`).
///
/// Falls back to a single flat (cursor-less) span when the combined text would need truncating to
/// fit `width` — splitting a *truncated* string into cursor-relative spans exactly is more
/// complexity than an edge case (an unusually long value on an unusually narrow terminal) is worth;
/// every other row already accepted plain truncation with no cursor shown, and still does here.
fn cursor_aware_spans(
    prefix: &str,
    value: &str,
    suffix: &str,
    cursor: Option<usize>,
    base: Style,
    width: usize,
) -> Vec<Span<'static>> {
    let Some(cursor) = cursor else {
        return vec![Span::styled(
            text::truncate(&format!("{prefix}{value}{suffix}"), width).into_owned(),
            base,
        )];
    };
    let full_width = text::width(prefix) + text::width(value) + text::width(suffix);
    if full_width > width {
        return vec![Span::styled(
            text::truncate(&format!("{prefix}{value}{suffix}"), width).into_owned(),
            base,
        )];
    }

    let cursor_style = base.add_modifier(Modifier::REVERSED);
    let chars: Vec<char> = value.chars().collect();
    let idx = cursor.min(chars.len());
    let mut spans = vec![Span::styled(prefix.to_string(), base)];
    if idx > 0 {
        spans.push(Span::styled(chars[..idx].iter().collect::<String>(), base));
    }
    if idx < chars.len() {
        spans.push(Span::styled(chars[idx].to_string(), cursor_style));
        if idx + 1 < chars.len() {
            spans.push(Span::styled(
                chars[idx + 1..].iter().collect::<String>(),
                base,
            ));
        }
    } else {
        // Cursor at the end of an otherwise-empty or fully-typed value: a highlighted blank cell.
        spans.push(Span::styled(" ".to_string(), cursor_style));
    }
    if !suffix.is_empty() {
        spans.push(Span::styled(suffix.to_string(), base));
    }
    spans
}

fn render_server_form(
    f: &mut Frame,
    area: Rect,
    draft: &loxia_core::state::settings::ServerDraft,
    theme: &Theme,
    hits: &mut HitMap,
) {
    use loxia_core::reducer::settings::server_draft_fields;
    use loxia_core::state::settings::ServerDraftField;

    let end_y = area.y + area.height;
    let mut y = area.y;
    let fields = server_draft_fields(draft);

    // `11-07`(field fix): the value shown for whichever field is under active edit, plus its
    // cursor's character index — `None` for every other field, which renders as one plain span
    // exactly as before.
    let live = |editing_this: bool, committed: &str| -> (String, Option<usize>) {
        if editing_this {
            let buf = draft.text_buf.as_ref();
            (
                buf.map(|b| b.text.clone()).unwrap_or_default(),
                buf.map(|b| b.cursor),
            )
        } else {
            (committed.to_string(), None)
        }
    };

    for (i, field) in fields.iter().enumerate() {
        if y >= end_y {
            break;
        }
        let is_cursor = i == draft.field;
        let editing_this = is_cursor && draft.text_buf.is_some();

        let (prefix, value, cursor, suffix) = match field {
            ServerDraftField::Name => {
                let (v, c) = live(editing_this, &draft.name);
                ("name: ", v, c, String::new())
            }
            ServerDraftField::Protocol => {
                // Never text-edited (`editing_this` is always `false` here — `Protocol` has no
                // `text_buf` path at all), so this is always the plain, cursor-less branch.
                let hint = if is_cursor {
                    "   [\u{2190}\u{2192}] change"
                } else {
                    ""
                };
                ("protocol: ", draft.protocol.clone(), None, hint.to_string())
            }
            ServerDraftField::Host => {
                let (v, c) = live(editing_this, &draft.host);
                ("host: ", v, c, String::new())
            }
            ServerDraftField::Port => {
                let (v, c) = live(editing_this, &draft.port);
                ("port: ", v, c, String::new())
            }
            ServerDraftField::Username => {
                let (v, c) = live(editing_this, &draft.username);
                ("username: ", v, c, String::new())
            }
            ServerDraftField::Password => {
                let (v, c) = live(
                    editing_this,
                    &masked(&draft.password, draft.reveal_password),
                );
                let hint = if is_cursor { "   [Ctrl+E] reveal" } else { "" };
                ("password: ", v, c, hint.to_string())
            }
            ServerDraftField::Header(idx) => {
                let (name, value) = &draft.headers[*idx];
                let revealed = draft.reveal_header == Some(*idx);
                let hint = if is_cursor {
                    "   [Ctrl+E] reveal  [x] remove"
                } else {
                    ""
                };
                (
                    "  ",
                    format!("{name} = {}", masked(value, revealed)),
                    None,
                    hint.to_string(),
                )
            }
            ServerDraftField::NewHeaderName => {
                let (v, c) = live(editing_this, &draft.new_header_name);
                ("new header name: ", v, c, String::new())
            }
            ServerDraftField::NewHeaderValue => {
                let (v, c) = live(editing_this, &draft.new_header_value);
                ("new header value: ", v, c, String::new())
            }
            ServerDraftField::AddHeaderAction => {
                ("[ add header ]", String::new(), None, String::new())
            }
            // Which address the protocol/host/port/header rows below are about. Named rather than
            // numbered past the first, since "primary" is the one a user thinks in terms of and the
            // rest are simply the other ways in.
            ServerDraftField::Endpoint => {
                let total = draft.endpoints.len();
                let label = if draft.endpoint == 0 {
                    "primary".to_string()
                } else {
                    format!("fallback {}", draft.endpoint)
                };
                let hint = if total > 1 {
                    format!("  [←→] of {total}   [x] remove")
                } else {
                    String::new()
                };
                ("address: ", label, None, hint)
            }
            ServerDraftField::AddEndpointAction => (
                "[ add another address ]",
                String::new(),
                None,
                String::new(),
            ),
            ServerDraftField::TestConnectionAction => {
                let label = if draft.testing {
                    "[ testing connection\u{2026} ]"
                } else {
                    "[ test connection ]"
                };
                (label, String::new(), None, String::new())
            }
            ServerDraftField::SaveAction => ("[ save ]", String::new(), None, String::new()),
        };

        let row_style = if is_cursor {
            style::selection(theme, true)
        } else {
            style::style(theme, Role::Fg)
        };
        let row_area = Rect::new(area.x, y, area.width, 1);
        hits.push(row_area, HitTarget::SettingsRow(i));
        f.render_widget(
            Paragraph::new(Line::from(cursor_aware_spans(
                prefix,
                &value,
                &suffix,
                cursor,
                row_style,
                area.width as usize,
            ))),
            row_area,
        );
        y += 1;

        // `header_error` belongs to the "add header" action specifically, not to whichever row
        // the cursor is currently on — it can only ever have been set by pressing Enter there.
        if matches!(field, ServerDraftField::AddHeaderAction)
            && let Some(err) = &draft.header_error
            && y < end_y
        {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    text::truncate(&format!("  {err}"), area.width as usize).into_owned(),
                    style::style(theme, Role::Error),
                ))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
        if matches!(field, ServerDraftField::TestConnectionAction)
            && let Some(result) = &draft.test_result
            && y < end_y
        {
            let (message, role) = match result {
                Ok(msg) => (msg.clone(), Role::Success),
                Err(msg) => (msg.clone(), Role::Error),
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    text::truncate(&format!("  {message}"), area.width as usize).into_owned(),
                    style::style(theme, role),
                ))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
    }
}

fn sort_field_label(field: loxia_core::config::SortField) -> &'static str {
    use loxia_core::config::SortField;
    match field {
        SortField::Name => "Name",
        SortField::Artist => "Artist",
        SortField::AlbumArtist => "Album Artist",
        SortField::Album => "Album",
        SortField::Year => "Year",
        SortField::TrackNumber => "Track Number",
        SortField::Genre => "Genre",
        SortField::DateAdded => "Date Added",
    }
}

fn direction_arrow(direction: loxia_core::config::Direction) -> char {
    match direction {
        loxia_core::config::Direction::Asc => '\u{2191}',
        loxia_core::config::Direction::Desc => '\u{2193}',
    }
}

/// `11-04`: the sort-profile editor — a flat list of `config.sorting.profiles`, with only the
/// focused one's own rules expanded (the wireframe's single `▶` marker), a live preview of the
/// first five queue tracks under it, and a footer hint. No `HitTarget`s are registered here —
/// keyboard only, the same deliberate scoping choice `11-02`'s keymap editor already made for its
/// own per-row actions (`docs/12-decisions.md`).
fn render_sort_editor(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    editor: &loxia_core::state::settings::SortProfileEditorState,
    theme: &Theme,
) {
    let end_y = area.y + area.height;
    let mut y = area.y;

    // A brand-new profile being named (`SortEditorNew`) isn't in `profiles` yet, so it has no
    // existing row to hang its name input on. Render it as its own row — otherwise, with an empty
    // list (the "deleted everything" case), the input box was invisible and typing seemed to do
    // nothing (`docs/12-decisions.md`).
    if editor.creating
        && let Some(buf) = &editor.name_buf
    {
        f.render_widget(
            Paragraph::new(Line::from(cursor_aware_spans(
                "\u{25b6} ",
                &buf.text,
                "  (new profile — Enter to create)",
                Some(buf.cursor),
                style::selection(theme, true),
                area.width as usize,
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    } else if state.config.sorting.profiles.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no sort profiles configured — press [n] to add one, or [R] to restore the defaults",
                style::fg(theme, Role::Dim),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }

    for (i, profile) in state.config.sorting.profiles.iter().enumerate() {
        if y >= end_y {
            break;
        }
        let is_focused_profile = i == editor.profile_cursor;
        let marker = if is_focused_profile {
            "\u{25b6} "
        } else {
            "  "
        };
        // Only a *rename* (`!creating`) shows its buffer inline on the profile's own row; a new
        // profile is rendered as its own dedicated row above, not by overwriting some existing
        // profile's name.
        let (name, cursor) = if is_focused_profile
            && !editor.creating
            && let Some(buf) = &editor.name_buf
        {
            (buf.text.clone(), Some(buf.cursor))
        } else {
            (profile.name.clone(), None)
        };
        let header_style = if is_focused_profile && editor.rule_cursor.is_none() {
            style::selection(theme, true)
        } else {
            style::style(theme, Role::Fg)
        };
        f.render_widget(
            Paragraph::new(Line::from(cursor_aware_spans(
                marker,
                &name,
                "",
                cursor,
                header_style,
                area.width as usize,
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;

        if is_focused_profile {
            for (r, rule) in profile.rules.iter().enumerate() {
                if y >= end_y {
                    break;
                }
                let is_rule_focused = editor.rule_cursor == Some(r);
                let text = format!(
                    "    {}. {:<14} {}",
                    r + 1,
                    sort_field_label(rule.field),
                    direction_arrow(rule.direction)
                );
                let rule_style = if is_rule_focused {
                    style::selection(theme, true)
                } else {
                    style::fg(theme, Role::Dim)
                };
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        text::truncate(&text, area.width as usize).into_owned(),
                        rule_style,
                    ))),
                    Rect::new(area.x, y, area.width, 1),
                );
                y += 1;
            }
        }
    }
    y += 1;

    if let Some(error) = &editor.error
        && y < end_y
    {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(error, area.width as usize).into_owned(),
                style::style(theme, Role::Error),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }

    // "With a queue loaded, the section shows the first five tracks as they would be ordered
    // under the focused profile, updating as rules change" (this task's own spec) — computed
    // fresh here from `state.queue`, never cached: there is nothing else to keep in sync.
    if y < end_y
        && let Some(profile) = state.config.sorting.profiles.get(editor.profile_cursor)
    {
        let mut preview: Vec<&loxia_core::model::Track> =
            state.queue.entries.iter().map(|e| &e.track).collect();
        preview.sort_by(|a, b| loxia_core::queue::sort::compare(a, b, profile));
        preview.truncate(5);
        if preview.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "preview: queue is empty",
                    style::fg(theme, Role::Dim),
                ))),
                Rect::new(area.x, y, area.width, 1),
            );
        } else {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "preview:",
                    style::fg(theme, Role::Dim),
                ))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
            for track in preview {
                if y >= end_y {
                    break;
                }
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        text::truncate(&format!("  {}", track.name), area.width as usize)
                            .into_owned(),
                        style::fg(theme, Role::Dim),
                    ))),
                    Rect::new(area.x, y, area.width, 1),
                );
                y += 1;
            }
        }
    }

    if end_y > area.y {
        let footer_y = end_y - 1;
        let footer = "[n] New  [r] Rename  [x] Delete  [R] Restore defaults  [a] Add rule  [d] Delete rule  [\u{2195}] Reorder  [Enter] Apply";
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(footer, area.width as usize).into_owned(),
                style::fg(theme, Role::Dim),
            ))),
            Rect::new(area.x, footer_y, area.width, 1),
        );
    }
}

/// `11-05`: the EQ-preset editor — a single flat list (`reducer::settings::eq_preset_rows`, the
/// fixed factory names first, then `config.equalizer.custom_presets`), factory rows marked
/// `(factory)` with no gains shown (they're read-only and this task's own wireframe shows no
/// numbers for them either), custom rows showing all ten gains inline. No `HitTarget`s — keyboard
/// only, the same choice `11-04`'s sort-profile editor already made.
fn render_eq_editor(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    editor: &loxia_core::state::settings::EqPresetEditorState,
    theme: &Theme,
) {
    let end_y = area.y + area.height;
    let mut y = area.y;
    let rows = loxia_core::reducer::settings::eq_preset_rows(&state.config);

    for (i, (name, is_factory)) in rows.iter().enumerate() {
        if y >= end_y {
            break;
        }
        let is_focused = i == editor.cursor;
        let marker = if is_focused { "\u{25b6} " } else { "  " };
        let style = if is_focused {
            style::selection(theme, true)
        } else {
            style::style(theme, Role::Fg)
        };

        let editing = is_focused.then_some(editor.name_buf.as_ref()).flatten();
        let spans = if let Some(buf) = editing {
            // While actively renaming/naming, the gains-column alignment below is dropped in
            // favour of showing a real, movable cursor — a brief, purely cosmetic trade-off for
            // the duration of typing (`docs/12-decisions.md`).
            cursor_aware_spans(
                marker,
                &buf.text,
                "",
                Some(buf.cursor),
                style,
                area.width as usize,
            )
        } else {
            let text = if *is_factory {
                format!("{marker}{name:<28} (factory)")
            } else if let Some(preset) = state
                .config
                .equalizer
                .custom_presets
                .iter()
                .find(|p| p.name == *name)
            {
                let gains: Vec<String> = preset.gains.iter().map(|g| format!("{g:+.1}")).collect();
                format!("{marker}{name:<28} {}", gains.join(" "))
            } else {
                format!("{marker}{name}")
            };
            vec![Span::styled(
                text::truncate(&text, area.width as usize).into_owned(),
                style,
            )]
        };
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }
    y += 1;

    if let Some(error) = &editor.error
        && y < end_y
    {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(error, area.width as usize).into_owned(),
                style::style(theme, Role::Error),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
    }

    if end_y > area.y {
        let footer_y = end_y - 1;
        let footer = "[s] Save current as new  [r] Rename  [x] Delete  [e] Open equalizer";
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(footer, area.width as usize).into_owned(),
                style::fg(theme, Role::Dim),
            ))),
            Rect::new(area.x, footer_y, area.width, 1),
        );
    }
}

/// `field`'s own dotted path (`ConfigWarning::field`, e.g. `"ui.theme"`, `"cache.rolling_max_gb"`)
/// against which section it belongs in — a plain prefix match against the same field names
/// `reducer::settings::rows_for_section` groups under each section.
fn warning_belongs_to_section(field: &str, section: SettingsSection) -> bool {
    match section {
        SettingsSection::Servers => {
            field == "active_server" || field == "servers" || field.starts_with("servers.")
        }
        SettingsSection::Audio => field.starts_with("audio."),
        SettingsSection::Cache => field.starts_with("cache."),
        SettingsSection::Transcode => field.starts_with("transcode."),
        SettingsSection::Interface => field.starts_with("ui.") || field.starts_with("logging."),
        SettingsSection::Sorting => field.starts_with("sorting."),
        SettingsSection::Equalizer => field.starts_with("equalizer."),
        SettingsSection::Keybindings => field == "keybindings" || field.starts_with("keybindings."),
        SettingsSection::About => false,
    }
}

/// `THIRD_PARTY_LICENSES.md`, embedded at compile time (`docs/11-packaging.md` §7 item 6: "the
/// file is embedded with `include_str!` so it can never drift from the shipped copy") — never
/// read from disk at runtime, so a corrupted or missing install-time copy can't blank the pane.
const THIRD_PARTY_LICENSES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../THIRD_PARTY_LICENSES.md"
));

fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{bytes} B")
    }
}

fn primary_url(state: &AppState) -> String {
    state
        .config
        .servers
        .iter()
        .find(|s| s.id == state.config.active_server)
        .map(|s| s.url.clone())
        .unwrap_or_default()
}

fn libmpv_line(status: &LibmpvStatus) -> String {
    match status {
        LibmpvStatus::NotLoaded(reason) => format!("libmpv       not loaded ({reason})"),
        LibmpvStatus::Loaded { version, path } => {
            format!("libmpv       {version} (loaded from {path})")
        }
    }
}

/// `docs/07-ui-spec.md` §9 / this task's own wireframe: version, licence, libmpv status, and the
/// config/cache/download paths and sizes. The wireframe also shows a `Build` line (rustc version,
/// target triple, build date) and a `Terminal` line (graphics protocol, cell dimensions) — neither
/// is in this task's own Acceptance list, and both would need new plumbing this task's Files list
/// doesn't call for (a `build.rs` for the former, threading the negotiated terminal size into
/// `AppState` for the latter); omitted rather than guessed, see `docs/12-decisions.md`.
fn render_about(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    if state.settings.about_licences_open {
        render_licences_pane(f, area, state, theme);
        return;
    }

    let mut lines: Vec<String> = vec![
        format!("loxia {}", env!("CARGO_PKG_VERSION")),
        "A terminal music client for Emby".to_string(),
        String::new(),
        "Licence      GPL-3.0-or-later".to_string(),
        libmpv_line(&state.about.libmpv),
    ];

    // Which way in is currently being used. With a fallback address configured this is the only
    // place that says whether the LAN path or the round trip through a proxy is in play — which is
    // usually the answer to "why is the library slow today" (`docs/12-decisions.md`).
    if !state.active_endpoint.is_empty() {
        let fallbacks = state
            .config
            .servers
            .iter()
            .find(|s| s.id == state.config.active_server)
            .map(|s| s.fallbacks.len())
            .unwrap_or(0);
        let via = if fallbacks > 0 && state.active_endpoint != primary_url(state) {
            "  (fallback)"
        } else {
            ""
        };
        lines.push(format!("Connected    {}{via}", state.active_endpoint));
    }

    if let Some(paths) = &state.paths {
        lines.push(format!("Config       {}", paths.config_file().display()));
        let used = state.cache_stats.audio_cache_bytes + state.cache_stats.image_cache_bytes;
        let budget_bytes = (state.config.cache.rolling_max_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        lines.push(format!(
            "Cache        {}  ({} / {})",
            paths.cache_root().display(),
            format_bytes(used),
            format_bytes(budget_bytes)
        ));
        let track_noun = if state.cache_stats.pinned_count == 1 {
            "track"
        } else {
            "tracks"
        };
        lines.push(format!(
            "Downloads    {}  ({}, {} {track_noun})",
            paths.downloads_root().display(),
            format_bytes(state.cache_stats.download_bytes),
            state.cache_stats.pinned_count
        ));
    }

    lines.push(String::new());
    lines.push("[l] Third-party licences   [d] Copy diagnostics".to_string());

    for (i, line) in lines.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(line, area.width as usize).into_owned(),
                style::style(theme, Role::Fg),
            ))),
            Rect::new(area.x, area.y + i as u16, area.width, 1),
        );
    }
}

/// `[l]`'s scrollable pane. `about_licences_scroll` is clamped only against zero by the reducer
/// (`reducer::settings::about_licences_scroll`) — it has no access to this embedded text's actual
/// line count (a `loxia-tui`-only constant), so the upper bound is clamped here, purely visually,
/// on every render instead.
fn render_licences_pane(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let all_lines: Vec<&str> = THIRD_PARTY_LICENSES.lines().collect();
    if all_lines.is_empty() {
        return;
    }
    // Clamped so the *last full page* stays on screen at the end, not just the final single line
    // (which `saturating_sub(1)` alone would leave, with the rest of the pane blank).
    let max_start = all_lines.len().saturating_sub(area.height as usize);
    let start = state.settings.about_licences_scroll.min(max_start);

    for (i, line) in all_lines[start..]
        .iter()
        .take(area.height as usize)
        .enumerate()
    {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text::truncate(line, area.width as usize).into_owned(),
                style::style(theme, Role::Fg),
            ))),
            Rect::new(area.x, area.y + i as u16, area.width, 1),
        );
    }
}

/// Total row count `render_row`'s own two-line-per-control layout (label/value, then description)
/// needs for `section` — used only by tests that want to size a `TestBackend` generously enough
/// to see every row.
#[cfg(test)]
fn min_height_for(section: SettingsSection, cfg: &loxia_core::config::Config) -> u16 {
    const ROWS_PER_CONTROL: u16 = 2;
    2 + (rows_for_section(section, cfg, &[]).len() as u16) * ROWS_PER_CONTROL
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::reducer::settings::apply;
    use loxia_core::state::nav::Tab;
    use loxia_core::test_support::fixtures;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn settings_state() -> AppState {
        let mut state = AppState {
            keymap: loxia_core::keymap::KeyMap::defaults(),
            ..AppState::default()
        };
        state.nav.active_tab = Tab::Settings;
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

    #[test]
    fn renders_every_section_label() {
        let state = settings_state();
        let (rendered, _) = draw_at(100, 30, &state);
        for section in SettingsSection::ALL {
            assert!(rendered.contains(section.label()), "{section:?}");
        }
    }

    #[test]
    fn secret_field_masked_by_default() {
        let mut state = settings_state();
        state.settings.section = SettingsSection::Servers;
        state.config.servers.push(loxia_core::config::ServerConfig {
            id: "srv1".to_string(),
            access_token: "super-secret-token".to_string(),
            ..Default::default()
        });
        state.config.active_server = "srv1".to_string();
        let height = min_height_for(SettingsSection::Servers, &state.config) + 4;
        let (rendered, _) = draw_at(100, height, &state);
        assert!(!rendered.contains("super-secret-token"));
        assert!(
            rendered.contains("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}")
        );
    }

    #[test]
    fn secret_field_reveal_toggle() {
        let mut state = settings_state();
        state.settings.section = SettingsSection::Servers;
        state.settings.reveal_secret = true;
        state.config.servers.push(loxia_core::config::ServerConfig {
            id: "srv1".to_string(),
            access_token: "super-secret-token".to_string(),
            ..Default::default()
        });
        state.config.active_server = "srv1".to_string();
        let height = min_height_for(SettingsSection::Servers, &state.config) + 4;
        let (rendered, _) = draw_at(100, height, &state);
        assert!(rendered.contains("super-secret-token"));
    }

    /// `11-01`: `secret_never_in_snapshot` — `settings_snapshot_per_section`'s own Servers
    /// snapshot is taken with `reveal_secret` at its default (`false`), so its persisted `.snap`
    /// file can never contain a real token; this asserts that same default-state rendering
    /// (never `insta::assert_snapshot!`'d here directly — a real snapshot file would itself be
    /// the very thing this rule forbids, were the token ever actually present in it).
    #[test]
    fn secret_never_in_snapshot() {
        let mut state = settings_state();
        state.settings.section = SettingsSection::Servers;
        assert!(
            !state.settings.reveal_secret,
            "the default, snapshotted state"
        );
        state.config.servers.push(loxia_core::config::ServerConfig {
            id: "srv1".to_string(),
            access_token: "super-secret-token".to_string(),
            ..Default::default()
        });
        state.config.active_server = "srv1".to_string();
        let height = min_height_for(SettingsSection::Servers, &state.config) + 4;
        let (rendered, _) = draw_at(100, height, &state);
        assert!(!rendered.contains("super-secret-token"));
    }

    #[test]
    fn config_warnings_render_in_section() {
        let mut state = settings_state();
        state.settings.section = SettingsSection::Cache;
        state
            .config_warnings
            .push(loxia_core::config::ConfigWarning {
                field: "cache.rolling_max_gb".to_string(),
                message: "rolling_max_gb must be positive; reset to 5".to_string(),
                severity: loxia_core::config::Severity::Warning,
            });
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("rolling_max_gb must be positive"));
    }

    #[test]
    fn config_warnings_do_not_leak_into_other_sections() {
        let mut state = settings_state();
        state.settings.section = SettingsSection::Audio;
        state
            .config_warnings
            .push(loxia_core::config::ConfigWarning {
                field: "cache.rolling_max_gb".to_string(),
                message: "rolling_max_gb must be positive; reset to 5".to_string(),
                severity: loxia_core::config::Severity::Warning,
            });
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(!rendered.contains("rolling_max_gb must be positive"));
    }

    /// `11-01`: `keybinding_conflict_badge_shown`.
    #[test]
    fn keybinding_conflict_badge_shown() {
        let mut state = settings_state();
        state.settings.section = SettingsSection::Keybindings;
        // A real conflict: bind both `PlayPause` and `Stop` to the same chord.
        let mut raw: std::collections::BTreeMap<String, String> = Default::default();
        raw.insert("play_pause".to_string(), "space".to_string());
        raw.insert("stop".to_string(), "space".to_string());
        let (keymap, _) = loxia_core::keymap::KeyMap::from_config(&raw);
        state.keymap = keymap;
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("conflict"));
    }

    #[test]
    fn no_conflict_badge_with_default_keymap() {
        let state = settings_state();
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(!rendered.contains("conflict"));
    }

    #[test]
    fn error_shown_beneath_the_row() {
        let mut state = settings_state();
        state.settings.section = SettingsSection::Cache;
        state.settings.error = Some((1, "value rejected: must be positive".to_string()));
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("value rejected: must be positive"));
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let state = settings_state();
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    /// `11-01`: `settings_snapshot_per_section` — one per section.
    #[test]
    fn settings_snapshot_per_section() {
        for section in SettingsSection::ALL {
            let mut state = settings_state();
            state.settings.section = section;
            let height = min_height_for(section, &state.config).max(24) + 4;
            let (rendered, _) = draw_at(100, height, &state);
            insta::assert_snapshot!(format!("settings_snapshot_{}", section.label()), rendered);
        }
    }

    // -----------------------------------------------------------------------------------------
    // `11-03`: server profiles sub-view.
    // -----------------------------------------------------------------------------------------

    fn server(id: &str, name: &str, url: &str) -> loxia_core::config::ServerConfig {
        loxia_core::config::ServerConfig {
            id: id.to_string(),
            name: name.to_string(),
            url: url.to_string(),
            user_id: "user-1".to_string(),
            access_token: "tok".to_string(),
            device_id: format!("device-{id}"),
            custom_headers: Default::default(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    fn server_editor_state() -> AppState {
        use loxia_core::state::settings::ServerEditorState;
        let mut state = settings_state();
        state.settings.section = SettingsSection::Servers;
        state.settings.server_editor = Some(ServerEditorState::default());
        state
    }

    #[test]
    fn list_marks_the_active_server() {
        let mut state = server_editor_state();
        state
            .config
            .servers
            .push(server("a", "Server A", "https://a.example.com"));
        state
            .config
            .servers
            .push(server("b", "Server B", "https://b.example.com"));
        state.config.active_server = "b".to_string();
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("\u{2022} Server B"));
        assert!(rendered.contains("  Server A"));
    }

    #[test]
    fn empty_list_shows_add_hint() {
        let state = server_editor_state();
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("no servers configured"));
        assert!(rendered.contains("[a]"));
    }

    #[test]
    fn list_shows_the_removal_reason_and_delete_flag() {
        let mut state = server_editor_state();
        state
            .config
            .servers
            .push(server("a", "Server A", "https://a.example.com"));
        state.config.active_server = "a".to_string();
        if let Some(editor) = &mut state.settings.server_editor {
            editor.error =
                Some("switch to another server before removing the active one".to_string());
            editor.delete_data_on_remove = true;
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("switch to another server before removing the active one"));
        assert!(rendered.contains("[x] delete data on remove"));
    }

    fn draft_state() -> AppState {
        let mut state = server_editor_state();
        apply(
            &mut state,
            loxia_core::action::SettingsAction::ServerEditorAddNew,
        );
        state
    }

    #[test]
    fn form_shows_name_url_username_and_masks_password() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.name = "My Server".to_string();
            draft.protocol = "https".to_string();
            draft.host = "example.com".to_string();
            draft.port = "8096".to_string();
            draft.username = "alice".to_string();
            draft.password = "s3cr3t-pw".to_string();
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("name: My Server"));
        assert!(rendered.contains("protocol: https"));
        assert!(rendered.contains("host: example.com"));
        assert!(rendered.contains("port: 8096"));
        assert!(rendered.contains("username: alice"));
        assert!(rendered.contains(
            "password: \u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
        ));
        assert!(!rendered.contains("s3cr3t-pw"));
    }

    #[test]
    fn protocol_field_shows_change_hint_only_when_focused() {
        let mut state = draft_state();
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(!rendered.contains("change"));

        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            // Found rather than counted — the form has since grown an address selector above it.
            draft.field = loxia_core::reducer::settings::server_draft_fields(draft)
                .iter()
                .position(|f| *f == loxia_core::state::settings::ServerDraftField::Protocol)
                .unwrap();
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("protocol: https"));
        assert!(rendered.contains("change"));
    }

    /// A real gap found in the field: without a visible, movable cursor, editing a field and
    /// merely having the row cursor rest on it looked identical, and there was no way to fix a
    /// typo except erasing everything after it. `cursor_aware_spans` is the shared fix; these
    /// exercise it directly, in isolation from any particular editor's own row layout.
    #[test]
    fn cursor_aware_spans_highlights_exactly_the_cursor_cell() {
        let base = Style::default();
        let spans = cursor_aware_spans(">> ", "abc", "", Some(1), base, 80);
        // "a" plain, "b" reversed (the cursor), "c" plain — plus the prefix.
        let reversed: Vec<&Span> = spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::REVERSED))
            .collect();
        assert_eq!(reversed.len(), 1);
        assert_eq!(reversed[0].content, "b");
        let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full, ">> abc");
    }

    #[test]
    fn cursor_aware_spans_at_end_highlights_a_blank_cell() {
        let spans = cursor_aware_spans("", "abc", "", Some(3), Style::default(), 80);
        let reversed: Vec<&Span> = spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::REVERSED))
            .collect();
        assert_eq!(reversed.len(), 1);
        assert_eq!(reversed[0].content, " ");
    }

    #[test]
    fn cursor_aware_spans_none_is_one_plain_span() {
        let spans = cursor_aware_spans("label: ", "value", "", None, Style::default(), 80);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "label: value");
    }

    #[test]
    fn cursor_aware_spans_falls_back_to_plain_when_it_would_need_truncating() {
        let spans = cursor_aware_spans("", "abc", "", Some(1), Style::default(), 2);
        assert_eq!(spans.len(), 1, "no cursor cell once truncation is needed");
        assert_eq!(spans[0].content, "ab");
    }

    #[test]
    fn typing_into_server_draft_field_shows_a_movable_cursor() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.field = 0; // Name
            draft.text_buf = Some(loxia_core::state::settings::TextEdit {
                text: "abc".to_string(),
                cursor: 1,
            });
        }
        let (rendered, _) = draw_at(100, 30, &state);
        // The live buffer, not the (empty) committed `name`, is what's shown.
        assert!(rendered.contains("name: abc"));
    }

    #[test]
    fn plain_row_text_edit_shows_live_typed_value() {
        let mut state = settings_state();
        // "download directory" — the Audio section's own output driver/device rows are dropdowns
        // now, so this uses a row that is genuinely still free text.
        state.settings.section = SettingsSection::Cache;
        let rows = rows_for_section(SettingsSection::Cache, &state.config, &[]);
        state.settings.cursor = rows
            .iter()
            .position(|r| r.label == "download directory")
            .unwrap();
        apply(
            &mut state,
            loxia_core::action::SettingsAction::StartTextEdit,
        );
        apply(
            &mut state,
            loxia_core::action::SettingsAction::TextCursorHome,
        );
        apply(
            &mut state,
            loxia_core::action::SettingsAction::TextInput('X'),
        );
        // Before this fix, the row kept showing the stale committed "auto" while typing — nothing
        // ever reflected a keystroke until Enter committed it.
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("Xauto"), "{rendered}");
    }

    #[test]
    fn password_reveal_toggle_shows_it_in_the_clear() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.password = "s3cr3t-pw".to_string();
            draft.reveal_password = true;
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("password: s3cr3t-pw"));
    }

    #[test]
    fn header_values_masked() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.headers.push((
                "CF-Access-Client-Secret".to_string(),
                "abc123secret".to_string(),
            ));
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("CF-Access-Client-Secret"));
        assert!(!rendered.contains("abc123secret"));
        assert!(
            rendered.contains("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}")
        );
    }

    #[test]
    fn header_error_shown_beneath_add_header_action() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.header_error =
                Some("authorization is set by loxia and cannot be overridden".to_string());
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("authorization is set by loxia and cannot be overridden"));
    }

    #[test]
    fn test_result_shown_success_and_failure() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.test_result = Some(Ok("connected to Home Library (v4.8.0.80)".to_string()));
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("connected to Home Library (v4.8.0.80)"));

        let mut state2 = draft_state();
        if let Some(draft) = state2
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.test_result = Some(Err("the server could not be reached.".to_string()));
        }
        let (rendered2, _) = draw_at(100, 30, &state2);
        assert!(rendered2.contains("the server could not be reached."));
    }

    #[test]
    fn testing_in_flight_shows_a_progress_hint() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.testing = true;
        }
        let (rendered, _) = draw_at(100, 30, &state);
        assert!(rendered.contains("testing connection"));
    }

    #[test]
    fn server_editor_tiny_area_does_not_panic() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft
                .headers
                .push(("X-Custom".to_string(), "v".to_string()));
        }
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn servers_section_snapshot_editor() {
        let mut state = server_editor_state();
        state
            .config
            .servers
            .push(server("a", "Server A", "https://a.example.com"));
        state.config.active_server = "a".to_string();
        let (rendered, _) = draw_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn servers_section_snapshot_form_headers() {
        let mut state = draft_state();
        if let Some(draft) = state
            .settings
            .server_editor
            .as_mut()
            .and_then(|e| e.editing.as_mut())
        {
            draft.name = "My Server".to_string();
            draft.host = "example.com".to_string();
            draft
                .headers
                .push(("CF-Access-Client-Id".to_string(), "abc123".to_string()));
        }
        let (rendered, _) = draw_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    // -----------------------------------------------------------------------------------------
    // `11-04`: sort profile editor.
    // -----------------------------------------------------------------------------------------

    fn sort_profile(
        name: &str,
        rules: Vec<(loxia_core::config::SortField, loxia_core::config::Direction)>,
    ) -> loxia_core::config::SortProfile {
        loxia_core::config::SortProfile {
            name: name.to_string(),
            rules: rules
                .into_iter()
                .map(|(field, direction)| loxia_core::config::SortRule { field, direction })
                .collect(),
        }
    }

    fn sort_editor_state() -> AppState {
        use loxia_core::state::settings::SortProfileEditorState;
        let mut state = settings_state();
        state.settings.section = SettingsSection::Sorting;
        state.config.sorting.profiles.clear();
        state.config.sorting.default_queue_profile.clear();
        state.settings.sort_profile_editor = Some(SortProfileEditorState::default());
        state
    }

    #[test]
    fn sort_editor_empty_shows_add_hint() {
        let state = sort_editor_state();
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("no sort profiles configured"));
        assert!(rendered.contains("[n]"));
    }

    #[test]
    fn sort_editor_shows_the_new_profile_name_input_from_empty() {
        use loxia_core::state::settings::TextEdit;
        // The "deleted everything" case: no profiles, but a new one is being named. The input row
        // must be visible (the bug was that it wasn't, so typing seemed to do nothing).
        let mut state = sort_editor_state();
        let editor = state.settings.sort_profile_editor.as_mut().unwrap();
        editor.creating = true;
        editor.name_buf = Some(TextEdit::new("my new mix".to_string()));

        let (rendered, _) = draw_at(100, 24, &state);
        assert!(
            rendered.contains("my new mix"),
            "the new profile's name-in-progress must be on screen: {rendered}"
        );
        assert!(
            !rendered.contains("no sort profiles configured"),
            "the empty-state hint is replaced by the name input while creating"
        );
    }

    #[test]
    fn sort_editor_shows_focused_profiles_rules_only() {
        let mut state = sort_editor_state();
        state.config.sorting.profiles.push(sort_profile(
            "expanded",
            vec![(
                loxia_core::config::SortField::Year,
                loxia_core::config::Direction::Desc,
            )],
        ));
        state.config.sorting.profiles.push(sort_profile(
            "collapsed",
            vec![(
                loxia_core::config::SortField::Name,
                loxia_core::config::Direction::Asc,
            )],
        ));
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("\u{25b6} expanded"));
        assert!(rendered.contains("Year"));
        assert!(rendered.contains("collapsed"));
        // "collapsed" is not the focused profile, so its own rule must not be shown.
        let expanded_pos = rendered.find("expanded").unwrap();
        let collapsed_pos = rendered.find("collapsed").unwrap();
        let year_pos = rendered.find("Year").unwrap();
        assert!(expanded_pos < year_pos && year_pos < collapsed_pos);
    }

    #[test]
    fn fifth_rule_reason_shown_inline() {
        let mut state = sort_editor_state();
        state.settings.sort_profile_editor.as_mut().unwrap().error =
            Some("maximum 4 rules".to_string());
        state
            .config
            .sorting
            .profiles
            .push(sort_profile("p", vec![]));
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("maximum 4 rules"));
    }

    #[test]
    fn live_preview_reflects_rule_changes() {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let zeta = fixtures::track("Zeta", 1, &alb, &[&a]);
        let alpha = fixtures::track("Alpha", 2, &alb, &[&a]);
        let mut state = sort_editor_state();
        for (i, track) in [zeta, alpha].into_iter().enumerate() {
            state
                .queue
                .entries
                .push(loxia_core::state::queue::QueueEntry {
                    entry_id: loxia_core::model::QueueEntryId(i as u64),
                    track,
                    source: loxia_core::state::queue::QueueSource::Manual,
                    availability: loxia_core::state::queue::Availability::Remote,
                });
            state.queue.play_order.push(i);
        }
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![(
                loxia_core::config::SortField::Name,
                loxia_core::config::Direction::Asc,
            )],
        ));

        let (asc_rendered, _) = draw_at(100, 24, &state);
        let alpha_pos = asc_rendered.find("Alpha").unwrap();
        let zeta_pos = asc_rendered.find("Zeta").unwrap();
        assert!(alpha_pos < zeta_pos, "ascending by name: Alpha before Zeta");

        // Flip the rule's own direction — the preview must reorder without touching the queue.
        state.config.sorting.profiles[0].rules[0].direction = loxia_core::config::Direction::Desc;
        let (desc_rendered, _) = draw_at(100, 24, &state);
        let alpha_pos2 = desc_rendered.find("Alpha").unwrap();
        let zeta_pos2 = desc_rendered.find("Zeta").unwrap();
        assert!(
            zeta_pos2 < alpha_pos2,
            "descending by name: Zeta before Alpha"
        );
    }

    #[test]
    fn sort_editor_tiny_area_does_not_panic() {
        let mut state = sort_editor_state();
        state.config.sorting.profiles.push(sort_profile(
            "p",
            vec![(
                loxia_core::config::SortField::Name,
                loxia_core::config::Direction::Asc,
            )],
        ));
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn sorting_section_snapshot() {
        let state = sort_editor_state();
        let (rendered, _) = draw_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn sorting_section_snapshot_editing() {
        let mut state = sort_editor_state();
        state.config.sorting.profiles.push(sort_profile(
            "chronological_discog",
            vec![
                (
                    loxia_core::config::SortField::AlbumArtist,
                    loxia_core::config::Direction::Asc,
                ),
                (
                    loxia_core::config::SortField::Year,
                    loxia_core::config::Direction::Asc,
                ),
            ],
        ));
        state
            .settings
            .sort_profile_editor
            .as_mut()
            .unwrap()
            .rule_cursor = Some(1);
        let (rendered, _) = draw_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn sorting_section_snapshot_max_rules() {
        let mut state = sort_editor_state();
        state.config.sorting.profiles.push(sort_profile(
            "full",
            vec![
                (
                    loxia_core::config::SortField::AlbumArtist,
                    loxia_core::config::Direction::Asc,
                ),
                (
                    loxia_core::config::SortField::Year,
                    loxia_core::config::Direction::Asc,
                ),
                (
                    loxia_core::config::SortField::Album,
                    loxia_core::config::Direction::Asc,
                ),
                (
                    loxia_core::config::SortField::TrackNumber,
                    loxia_core::config::Direction::Asc,
                ),
            ],
        ));
        state.settings.sort_profile_editor.as_mut().unwrap().error =
            Some("maximum 4 rules".to_string());
        let (rendered, _) = draw_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    // -----------------------------------------------------------------------------------------
    // `11-05`: EQ preset manager.
    // -----------------------------------------------------------------------------------------

    fn eq_editor_state() -> AppState {
        use loxia_core::state::settings::EqPresetEditorState;
        let mut state = settings_state();
        state.settings.section = SettingsSection::Equalizer;
        state.settings.eq_preset_editor = Some(EqPresetEditorState::default());
        state
    }

    #[test]
    fn factory_presets_show_no_gains_and_are_marked() {
        let state = eq_editor_state();
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("flat"));
        assert!(rendered.contains("(factory)"));
    }

    #[test]
    fn custom_preset_shows_its_gains() {
        let mut state = eq_editor_state();
        state
            .config
            .equalizer
            .custom_presets
            .push(loxia_core::config::EqPreset {
                name: "my preset".to_string(),
                gains: [2.0, 2.0, 1.0, 0.0, 0.0, -1.0, -1.5, -2.0, -3.0, -4.0],
            });
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("my preset"));
        assert!(rendered.contains("+2.0"));
        assert!(rendered.contains("-4.0"));
    }

    #[test]
    fn eq_editor_error_shown() {
        let mut state = eq_editor_state();
        state.settings.eq_preset_editor.as_mut().unwrap().error =
            Some("factory presets cannot be modified".to_string());
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("factory presets cannot be modified"));
    }

    #[test]
    fn eq_editor_footer_shown() {
        let state = eq_editor_state();
        let (rendered, _) = draw_at(100, 24, &state);
        assert!(rendered.contains("Save current as new"));
        assert!(rendered.contains("Rename"));
        assert!(rendered.contains("Delete"));
        assert!(rendered.contains("Open equalizer"));
    }

    #[test]
    fn eq_editor_tiny_area_does_not_panic() {
        let mut state = eq_editor_state();
        state
            .config
            .equalizer
            .custom_presets
            .push(loxia_core::config::EqPreset {
                name: "p".to_string(),
                gains: [1.0; 10],
            });
        for w in 0..5u16 {
            for h in 0..5u16 {
                let _ = draw_at(w, h, &state);
            }
        }
    }

    #[test]
    fn equalizer_section_snapshot() {
        let mut state = eq_editor_state();
        state
            .config
            .equalizer
            .custom_presets
            .push(loxia_core::config::EqPreset {
                name: "night_listening_warm".to_string(),
                gains: [2.0, 2.0, 1.0, 0.0, 0.0, -1.0, -1.5, -2.0, -3.0, -4.0],
            });
        state.settings.eq_preset_editor.as_mut().unwrap().cursor =
            loxia_core::reducer::settings::eq_preset_rows(&state.config).len() - 1;
        let (rendered, _) = draw_at(100, 24, &state);
        insta::assert_snapshot!(rendered);
    }

    fn about_state() -> AppState {
        let mut state = settings_state();
        state.settings.section = SettingsSection::About;
        state
    }

    #[test]
    fn about_shows_version_and_licence() {
        let state = about_state();
        let (rendered, _) = draw_at(100, 12, &state);
        assert!(rendered.contains(&format!("loxia {}", env!("CARGO_PKG_VERSION"))));
        assert!(rendered.contains("GPL-3.0-or-later"));
    }

    #[test]
    fn about_shows_libmpv_version_and_path() {
        let mut state = about_state();
        state.about.libmpv = LibmpvStatus::Loaded {
            version: "0.35.1".to_string(),
            path: "/usr/lib/libmpv.so.2".to_string(),
        };
        let (rendered, _) = draw_at(100, 12, &state);
        assert!(rendered.contains("0.35.1"));
        assert!(rendered.contains("/usr/lib/libmpv.so.2"));
    }

    #[test]
    fn no_audio_shows_not_loaded() {
        let mut state = about_state();
        state.about.libmpv = LibmpvStatus::NotLoaded("--no-audio".to_string());
        let (rendered, _) = draw_at(100, 12, &state);
        assert!(rendered.contains("not loaded (--no-audio)"));
    }

    #[test]
    fn about_shows_cache_and_download_sizes() {
        let mut state = about_state();
        state.paths = Some(
            loxia_core::paths::Paths::resolve(
                &loxia_core::paths::FixedDirs::new(
                    Some("/tmp/loxia-test/config".into()),
                    Some("/tmp/loxia-test/cache".into()),
                    Some("/tmp/loxia-test/data".into()),
                    Some("/tmp/loxia-test/state".into()),
                ),
                &loxia_core::config::CacheConfig::default(),
            )
            .unwrap(),
        );
        state.config.cache.rolling_max_gb = 5.0;
        state.cache_stats.audio_cache_bytes = 2_100_000_000;
        state.cache_stats.download_bytes = 14_700_000_000;
        state.cache_stats.pinned_count = 312;
        let (rendered, _) = draw_at(100, 12, &state);
        assert!(rendered.contains("2.0 GB"), "{rendered}");
        assert!(rendered.contains("5.0 GB"), "{rendered}");
        assert!(rendered.contains("13.7 GB"), "{rendered}");
        assert!(rendered.contains("312 tracks"), "{rendered}");
    }

    #[test]
    fn about_snapshot() {
        let mut state = about_state();
        state.about.libmpv = LibmpvStatus::Loaded {
            version: "0.35.1".to_string(),
            path: "/usr/lib/libmpv.so.2".to_string(),
        };
        let (rendered, _) = draw_at(100, 14, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn licences_pane_scrolls() {
        let mut state = about_state();
        let (top, _) = draw_at(100, 10, &state);
        state.settings.about_licences_open = true;
        state.settings.about_licences_scroll = 5;
        let (scrolled, _) = draw_at(100, 10, &state);
        assert_ne!(top, scrolled);

        // Scrolling far past the end must clamp visually to the last full page, not panic or
        // leave the pane mostly blank.
        state.settings.about_licences_scroll = 100_000;
        let (far, _) = draw_at(100, 10, &state);
        assert!(
            far.contains("zvariant_utils"),
            "expected the licence text's last lines still visible: {far}"
        );
    }

    #[test]
    fn licences_snapshot() {
        let mut state = about_state();
        state.settings.about_licences_open = true;
        let (rendered, _) = draw_at(100, 10, &state);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn third_party_licenses_contains_mpv_notice_verbatim() {
        assert!(THIRD_PARTY_LICENSES.contains("THIRD-PARTY SOFTWARE NOTICE: libmpv"));
        assert!(THIRD_PARTY_LICENSES.contains("Copyright: © mpv project contributors."));
        assert!(THIRD_PARTY_LICENSES.contains("https://github.com/mpv-player/mpv"));
        assert!(THIRD_PARTY_LICENSES.contains(
            "libmpv and loxia are distributed in the hope that they will be useful, but"
        ));
        assert!(THIRD_PARTY_LICENSES.contains("WITHOUT ANY WARRANTY"));
        assert!(
            THIRD_PARTY_LICENSES
                .contains("or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Lesser General Public")
        );
    }

    /// `THIRD_PARTY_LICENSES` is a `const &'static str` from `include_str!`, evaluated at compile
    /// time — there is no `std::fs::read`/`read_to_string` call anywhere in `render_licences_pane`
    /// or `render_about`, so a missing or corrupted on-disk copy at runtime cannot blank the pane
    /// (`docs/11-packaging.md` §7 item 6).
    #[test]
    fn third_party_licenses_is_embedded_not_read_from_disk() {
        assert!(!THIRD_PARTY_LICENSES.is_empty());
    }
}
