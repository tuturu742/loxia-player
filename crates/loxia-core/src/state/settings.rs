//! Settings tab state (`11-01`, `docs/07-ui-spec.md` §9): which section/row has focus, and any
//! in-progress text edit. The actual row *contents* (which `Control` each row is, and its current
//! value) are never stored here — `reducer::settings::rows_for_section` rebuilds that fresh from
//! `Config` on every render/action, so there is nothing here that could go stale relative to it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SettingsSection {
    #[default]
    Servers,
    Audio,
    Cache,
    Transcode,
    Interface,
    Sorting,
    Equalizer,
    Keybindings,
    About,
}

impl SettingsSection {
    /// Left-pane order (`docs/07-ui-spec.md` §9's own section list).
    pub const ALL: [SettingsSection; 9] = [
        SettingsSection::Servers,
        SettingsSection::Audio,
        SettingsSection::Cache,
        SettingsSection::Transcode,
        SettingsSection::Interface,
        SettingsSection::Sorting,
        SettingsSection::Equalizer,
        SettingsSection::Keybindings,
        SettingsSection::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsSection::Servers => "Servers",
            SettingsSection::Audio => "Audio",
            SettingsSection::Cache => "Cache",
            SettingsSection::Transcode => "Transcode",
            SettingsSection::Interface => "Interface",
            SettingsSection::Sorting => "Sorting",
            SettingsSection::Equalizer => "Equalizer",
            SettingsSection::Keybindings => "Keybindings",
            SettingsSection::About => "About",
        }
    }

    pub fn next(self) -> SettingsSection {
        let idx = Self::ALL.iter().position(|&s| s == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> SettingsSection {
        let idx = Self::ALL.iter().position(|&s| s == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// A text buffer under active edit, with a movable **character** cursor — shared by every place
/// in Settings a value is typed in place: the plain row editor (`SettingsState.editing`), the
/// server-profile draft's own focused field (`ServerDraft.text_buf`), and the sort-profile/
/// EQ-preset editors' own name buffers. Found missing in the field: without a real cursor
/// position, every one of these could only ever be edited by backspacing from the end — there was
/// no way to fix a typo in the middle short of erasing everything after it (`docs/12-decisions.md`).
///
/// `cursor` counts **characters**, not bytes, so it can never land mid-codepoint; `insert`/
/// `backspace`/`delete_forward` convert to a byte offset only at the point of the actual `String`
/// mutation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextEdit {
    pub text: String,
    pub cursor: usize,
}

impl TextEdit {
    /// Starts editing `text` with the cursor at its end — matching every existing "start editing
    /// this field" call site's own prior behaviour (a fresh edit always began by typing more,
    /// never by inserting before what's already there).
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        TextEdit { text, cursor }
    }

    fn byte_offset(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    pub fn insert(&mut self, c: char) {
        let at = self.byte_offset(self.cursor);
        self.text.insert(at, c);
        self.cursor += 1;
    }

    /// Deletes the character *before* the cursor (`Backspace`) — a no-op at the start of the text.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let at = self.byte_offset(self.cursor);
        self.text.remove(at);
    }

    /// Deletes the character *at* the cursor (`Delete`) — a no-op at the end of the text.
    pub fn delete_forward(&mut self) {
        if self.cursor >= self.text.chars().count() {
            return;
        }
        let at = self.byte_offset(self.cursor);
        self.text.remove(at);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.text.chars().count());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.chars().count();
    }
}

#[cfg(test)]
mod text_edit_tests {
    use super::TextEdit;

    #[test]
    fn insert_at_cursor_not_always_at_end() {
        let mut e = TextEdit::new("helloworld");
        e.cursor = 5; // between "hello" and "world"
        e.insert(' ');
        assert_eq!(e.text, "hello world");
        assert_eq!(e.cursor, 6);
    }

    #[test]
    fn backspace_removes_before_cursor_and_moves_left() {
        let mut e = TextEdit::new("hello");
        e.cursor = 3; // "hel|lo"
        e.backspace();
        assert_eq!(e.text, "helo");
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn backspace_at_start_is_a_no_op() {
        let mut e = TextEdit::new("hello");
        e.cursor = 0;
        e.backspace();
        assert_eq!(e.text, "hello");
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn delete_forward_removes_at_cursor_without_moving_it() {
        let mut e = TextEdit::new("hello");
        e.cursor = 1; // "h|ello"
        e.delete_forward();
        assert_eq!(e.text, "hllo");
        assert_eq!(e.cursor, 1);
    }

    #[test]
    fn delete_forward_at_end_is_a_no_op() {
        let mut e = TextEdit::new("hello");
        e.move_end();
        e.delete_forward();
        assert_eq!(e.text, "hello");
    }

    #[test]
    fn cursor_movement_clamps_at_both_ends() {
        let mut e = TextEdit::new("hi");
        e.move_left();
        e.move_left();
        e.move_left();
        assert_eq!(e.cursor, 0);
        e.move_end();
        e.move_right();
        e.move_right();
        assert_eq!(e.cursor, 2);
        e.move_home();
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn multibyte_characters_never_split() {
        // "héllo" — 'é' is a single character but two UTF-8 bytes; a byte-index cursor would
        // panic inserting/removing at a non-char-boundary offset here.
        let mut e = TextEdit::new("héllo");
        e.cursor = 2; // "hé|llo"
        e.insert('!');
        assert_eq!(e.text, "hé!llo");
        e.backspace();
        assert_eq!(e.text, "héllo");
        e.delete_forward();
        assert_eq!(e.text, "hélo");
    }
}

/// Ephemeral text-edit buffer for whichever `Control::Text` row is currently being edited —
/// `Some` only between `StartTextEdit` and `CommitTextEdit`/`CancelTextEdit`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsState {
    pub section: SettingsSection,
    /// Row index within the focused section's own row list (`reducer::settings::rows_for_section`).
    pub cursor: usize,
    pub editing: Option<TextEdit>,
    /// Whether the focused row's own secret `Text` field (`access_token`) is shown in the clear —
    /// `Ctrl+E` toggles this; always `false` when the cursor moves to a different row.
    pub reveal_secret: bool,
    /// `(row index, message)` — sixed by a rejected edit (`config::validate` changed the value
    /// back); cleared on the next successful edit or a cursor move.
    pub error: Option<(usize, String)>,
    /// `11-03`: `Some` while the Servers section shows the profile-management sub-view (opened
    /// via the "manage servers" `Control::Action` row, `11-01`) instead of its own normal row
    /// list. Lives alongside `editing`/`reveal_secret` above rather than replacing them — the
    /// ordinary Servers section rows (active-server picker, access-token field) are unaffected by
    /// this sub-view being open; they simply aren't rendered while it is.
    pub server_editor: Option<ServerEditorState>,
    /// `11-04`: as `server_editor`, for the Sorting section's own "manage sort profiles" row.
    pub sort_profile_editor: Option<SortProfileEditorState>,
    /// `11-05`: as `server_editor`, for the Equalizer section's own "manage EQ presets" row.
    pub eq_preset_editor: Option<EqPresetEditorState>,
    /// `11-07`: the About section's own `[l]` — a scrollable pane showing
    /// `THIRD_PARTY_LICENSES.md`'s embedded contents in place of the section's own row list
    /// (which is empty; `rows_for_section` returns nothing for `About`).
    pub about_licences_open: bool,
    /// Line offset into the licences text — `reducer::settings` never clamps this against the
    /// embedded text's actual length (it doesn't have that text; `loxia-tui` does), only against
    /// zero; the view clamps the upper bound purely visually when it renders.
    pub about_licences_scroll: usize,
    /// Whether focus is on the left-hand section (group) list rather than the focused section's own
    /// rows. `Esc` steps outward — rows → section list → the main tab sidebar
    /// (`NavState.sidebar_focused`); `Enter`/`→` step back inward (`docs/12-decisions.md`). `false`
    /// (rows focused) is the resting state on tab entry.
    pub section_list_focused: bool,
}

/// The EQ-preset management sub-view's own state — a single flat cursor over
/// `config::FACTORY_EQ_PRESET_NAMES` followed by `config.equalizer.custom_presets` (the same order
/// `reducer::settings::equalizer_rows`'s own active-preset dropdown already lists them in), no
/// two-level nesting: every preset here is a single row, unlike a sort profile's own rules.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EqPresetEditorState {
    pub cursor: usize,
    /// In-progress name buffer — `Some` while naming a brand-new preset (`creating: true`, `s`)
    /// or renaming the focused custom one (`creating: false`, `r`).
    pub name_buf: Option<TextEdit>,
    pub creating: bool,
    /// Captured the moment `s` is pressed, not re-read at commit time — "captures the current
    /// live gains" (this task's own spec) means *at the moment of the keystroke*, not whatever
    /// happens to still be playing once a name has been typed in and confirmed.
    pub captured_gains: Option<[f32; 10]>,
    pub error: Option<String>,
}

/// The sort-profile management sub-view's own state — a flat list of `config.sorting.profiles`,
/// with exactly one profile's own rules expanded at a time (the wireframe's own single `▶`
/// marker): `profile_cursor` names which one, `rule_cursor` is `None` while its header row itself
/// is focused, `Some(i)` while one of its rule rows is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SortProfileEditorState {
    pub profile_cursor: usize,
    pub rule_cursor: Option<usize>,
    /// In-progress name buffer — `Some` while creating a brand-new profile (`creating: true`) or
    /// renaming the focused one (`creating: false`), mirroring `ServerDraft::text_buf`'s own
    /// separate-from-`SettingsState::editing` shape (`11-03`), since a sort-profile name and an
    /// ordinary Settings row are never being edited at the same time either.
    pub name_buf: Option<TextEdit>,
    pub creating: bool,
    pub error: Option<String>,
}

/// The server-profile management sub-view's own state — a list of `config.servers`, or (while
/// `editing` is `Some`) the add/edit form for one of them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerEditorState {
    /// Focused row in the plain profile list.
    pub cursor: usize,
    pub editing: Option<ServerDraft>,
    /// "Removing any profile offers to delete its cached files and downloads, defaulting to no"
    /// (this task's own spec) — a toggle on the list sub-view itself, read at the moment removal
    /// actually runs, not a second confirmation step of its own.
    pub delete_data_on_remove: bool,
    /// Set when removing the *active* profile is refused ("disabled with reason") — this task's
    /// own spec asks for the reason to be shown, not just the action silently doing nothing.
    pub error: Option<String>,
}

/// Field indices within the add/edit form, in Tab-cycle order. Header rows are not included here
/// — they sit between `Password` and `NewHeaderName` in the *rendered* list, at a position that
/// depends on `headers.len()`; see `reducer::settings::server_draft_field_count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerDraftField {
    Name,
    /// Which address of this profile the form's protocol/host/port/header fields are editing:
    /// `primary`, or one of its fallbacks. `[←→]` selects, `+` adds one, `x` removes the selected
    /// fallback.
    ///
    /// Deliberately a *selector* rather than a nested sub-form: the whole existing form — including
    /// `Test connection` — then applies to whichever address is selected, and every field below
    /// keeps working exactly as it did, editing "the endpoint currently in view"
    /// (`docs/12-decisions.md`).
    Endpoint,
    /// `[←→]`/`Enter` cycles `http`/`https` — the one field in this form that isn't free text, so
    /// it never goes through `text_buf` at all (`reducer::settings::server_editor_cycle_protocol`).
    Protocol,
    Host,
    /// Free text, not `Control::Number` — a stray non-digit here is caught the same way a wrong
    /// host or scheme already is, by `Test connection` failing, not by inline validation
    /// (`docs/12-decisions.md`).
    Port,
    Username,
    Password,
    /// One of `draft.headers`' existing rows — `x` removes it; nothing else about it is editable
    /// (a wrong header is meant to be deleted and re-added, not patched in place).
    Header(usize),
    /// `Enter` adds another address for the same server, the same shape as `AddHeaderAction`.
    AddEndpointAction,
    NewHeaderName,
    NewHeaderValue,
    AddHeaderAction,
    TestConnectionAction,
    SaveAction,
}

/// One address of a profile, as the form holds it. The *selected* one's values live in
/// [`ServerDraft`]'s own `protocol`/`host`/`port`/`headers`; switching selection swaps them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerEndpointDraft {
    pub protocol: String,
    pub host: String,
    pub port: String,
    pub headers: Vec<(String, String)>,
}

/// A profile being added or edited — never written into `config.servers` until `Save` succeeds
/// (`ServerEditorSave`'s own reply handling), so an abandoned edit (`Esc`) never has anything to
/// undo.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerDraft {
    /// `None` while adding a brand-new profile; `Some(id)` while editing an existing one — the id
    /// itself is immutable once assigned (`config.servers[].id` is also how `active_server`
    /// refers to a profile, so letting it change out from under a save would be a real footgun).
    pub id: Option<String>,
    /// Stamped once, immediately, when the draft is created (`ServerEditorAddNew`/`ServerEditorEdit`)
    /// — "generated once per profile," not deferred to a later `config::validate()` pass
    /// (`docs/12-decisions.md`).
    pub device_id: String,
    pub name: String,
    /// `"http"` or `"https"` — always one of exactly those two, since the only way to change it
    /// is `server_editor_cycle_protocol` cycling through `URL_PROTOCOLS`, never free text.
    pub protocol: String,
    pub host: String,
    /// Empty means "no explicit port" — `reducer::settings::build_server_url` omits the `:port`
    /// suffix entirely rather than guessing a default from `protocol`, since Emby's own default
    /// (8096) isn't the HTTP/HTTPS default either way.
    pub port: String,
    pub username: String,
    pub password: String,
    /// Insertion order, not a `BTreeMap` — this task's own spec shows headers as an ordered list a
    /// user adds to and removes from, not an alphabetised one.
    pub headers: Vec<(String, String)>,
    pub field: usize,
    /// In-progress edit buffer for whichever text field `field` currently names — mirrors
    /// `SettingsState::editing`'s own shape, kept separate since a server draft and an ordinary
    /// Settings row can never both be under edit at once, but conflating the two fields would
    /// blur which one a given action is about.
    pub text_buf: Option<TextEdit>,
    pub new_header_name: String,
    pub new_header_value: String,
    pub header_error: Option<String>,
    pub reveal_password: bool,
    /// Which existing header row (if any) is currently shown in the clear — at most one at a
    /// time, the same "moving the cursor re-masks everything" rule `SettingsState::reveal_secret`
    /// already established.
    pub reveal_header: Option<usize>,
    /// Every address this profile has, index 0 the primary.
    ///
    /// The entry at `endpoint` is the one being edited, and its values are held in the draft's own
    /// `protocol`/`host`/`port`/`headers` rather than here — so every existing field, action and
    /// test in this form keeps working unchanged, and only selection has to move values in and out
    /// (`ServerDraft::select_endpoint`).
    pub endpoints: Vec<ServerEndpointDraft>,
    pub endpoint: usize,
    /// `Ok(message)`/`Err(message)` from the last `ServerEditorTestConnection` — cleared whenever
    /// any field changes, so a stale success never lingers next to newly-edited (and so
    /// once again untested) fields.
    pub test_result: Option<Result<String, String>>,
    /// A test or save is currently in flight — re-triggering either while this is `true` is a
    /// no-op (`docs/12-decisions.md`: no cancellation mechanism exists for an in-flight HTTP
    /// request, so the simplest correct behaviour is simply refusing to start a second one).
    pub testing: bool,
    /// `11-03`: `true` only for the in-flight request `ServerEditorSave` itself started —
    /// distinguishes "the reply that just arrived should also commit the draft" from a plain
    /// `ServerEditorTestConnection` reply, which must only ever update `test_result`.
    pub saving: bool,
}

impl ServerDraft {
    /// Moves the form onto endpoint `index`: the values on screen are written back to whichever
    /// endpoint they belong to, and the newly selected one's are loaded in their place.
    pub fn select_endpoint(&mut self, index: usize) {
        if index >= self.endpoints.len() {
            return;
        }
        self.stash_endpoint();
        self.endpoint = index;
        let selected = self.endpoints[index].clone();
        self.protocol = selected.protocol;
        self.host = selected.host;
        self.port = selected.port;
        self.headers = selected.headers;
        self.text_buf = None;
        self.reveal_header = None;
    }

    /// Copies the form's live protocol/host/port/headers back into the selected endpoint. Called
    /// before selection moves, and before a save reads the endpoint list.
    pub fn stash_endpoint(&mut self) {
        if let Some(slot) = self.endpoints.get_mut(self.endpoint) {
            slot.protocol = self.protocol.clone();
            slot.host = self.host.clone();
            slot.port = self.port.clone();
            slot.headers = self.headers.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_and_prev_wrap_around() {
        assert_eq!(SettingsSection::About.next(), SettingsSection::Servers);
        assert_eq!(SettingsSection::Servers.prev(), SettingsSection::About);
    }

    #[test]
    fn next_then_prev_is_identity() {
        for section in SettingsSection::ALL {
            assert_eq!(section.next().prev(), section);
        }
    }
}
