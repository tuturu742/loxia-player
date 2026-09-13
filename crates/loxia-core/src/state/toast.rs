//! Toast and ToastLevel (`10-13`, `docs/07-ui-spec.md` §13).

use jiff::SignedDuration;
use serde::{Deserialize, Serialize};

use crate::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    /// "Errors last longest because they are the ones a user needs time to read" (this task's own
    /// spec) — the only place these four numbers are named; `reducer::tick`'s expiry check reads
    /// this rather than a single shared constant.
    pub fn lifetime(self) -> SignedDuration {
        match self {
            ToastLevel::Info | ToastLevel::Success => SignedDuration::from_secs(3),
            ToastLevel::Warning => SignedDuration::from_secs(6),
            ToastLevel::Error => SignedDuration::from_secs(8),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Toast {
    /// Assigned by `AppState::toast` (`AppState::next_toast_id`) — stable identity distinct from
    /// `message` equality, which `toast`'s own deduplication already uses for "is this the same
    /// toast" instead.
    pub id: u64,
    pub message: String,
    pub level: ToastLevel,
    pub created_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_last_longest() {
        assert!(ToastLevel::Error.lifetime() > ToastLevel::Warning.lifetime());
        assert!(ToastLevel::Warning.lifetime() > ToastLevel::Info.lifetime());
        assert_eq!(ToastLevel::Info.lifetime(), ToastLevel::Success.lifetime());
    }

    /// `10-13`: `toast_lifetimes_by_level`.
    #[test]
    fn toast_lifetimes_by_level() {
        assert_eq!(ToastLevel::Info.lifetime(), SignedDuration::from_secs(3));
        assert_eq!(ToastLevel::Success.lifetime(), SignedDuration::from_secs(3));
        assert_eq!(ToastLevel::Warning.lifetime(), SignedDuration::from_secs(6));
        assert_eq!(ToastLevel::Error.lifetime(), SignedDuration::from_secs(8));
    }

    // --- 10-13: writing-rules audit -----------------------------------------------------------
    //
    // "A test iterating every literal toast string in the workspace" (this task's own spec) —
    // real toast text is constructed two ways: `AppState::toast(...)` (every reducer call site,
    // this crate's own `reducer/` modules — the only place that method is ever called), and a
    // bare `SystemEvent::Toast { message: "...", .. }` literal built directly by a worker in a
    // *different* crate (`loxia-cache::downloads`'s "disk full" toast) that still reaches the
    // exact same `AppState::toast` deduplication once the reducer dispatches it
    // (`reducer::mod`'s own handler for that variant). So this walks the whole workspace's
    // `crates/*/src`, not just this crate's own — `workers::network`'s WebSocket `DisplayMessage`
    // toast is the one `SystemEvent::Toast` construction site deliberately *not* matched by the
    // scan below: its `message` comes from `String`, not a literal (server-supplied text an
    // Emby-server admin writes, not this codebase's own copy) — grep confirms these are the only
    // three real construction sites as of this task.
    //
    // No `regex` dependency (not in the locked set, `docs/13-dependencies.md`) — plain string
    // scanning instead. This intentionally extracts only the *static* portions of a message (the
    // text around any `{placeholder}`), never what a runtime value later fills in — a dynamic
    // device/server/track name isn't something a writing-style rule can judge, and `EmbyError`'s
    // own display safety (never leaking a token/URL) is that type's own concern, not relitigated
    // here.

    /// Every literal string handed to `.toast(`, to a bare `SystemEvent::Toast { message: "...",
    /// .. }` construction, or to a helper function whose name ends in `_toast_text`
    /// (`reducer::player`'s `quality_toast_text`/`replay_gain_toast_text`, the only two such
    /// indirections that exist).
    fn all_toast_literals() -> Vec<(String, String)> {
        let mut out = Vec::new();
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("loxia-core lives at <workspace>/crates/loxia-core")
            .to_path_buf();
        let crates_dir = workspace_root.join("crates");
        // Excludes this scanner's own file: its doc comments illustrate the very patterns being
        // searched for (`SystemEvent::Toast { message: "...", .. }` as example text), which the
        // scan below would otherwise happily match against itself.
        let self_path = workspace_root.join(file!());
        for path in collect_rs_files(&crates_dir) {
            if path == self_path {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let label = path
                .strip_prefix(&crates_dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            for literal in extract_literals_after(&contents, ".toast(") {
                out.push((label.clone(), literal));
            }
            for literal in extract_toast_text_helper_literals(&contents) {
                out.push((label.clone(), literal));
            }
            for literal in extract_toast_struct_literals(&contents) {
                out.push((label.clone(), literal));
            }
        }
        out
    }

    /// A bare `SystemEvent::Toast { message: "...", .. }` construction (as opposed to the
    /// destructuring match arm `SystemEvent::Toast { message, level } =>`, which this never
    /// matches — there, `message` has no following `:`). Scoped to right after the enum path
    /// specifically, not a blind workspace-wide search for `message:`, which would also match
    /// unrelated structs that happen to share that field name.
    fn extract_toast_struct_literals(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = source;
        while let Some(pos) = rest.find("SystemEvent::Toast {") {
            let after = &rest[pos + "SystemEvent::Toast {".len()..];
            if let Some(msg_pos) = after.find("message:") {
                let after_msg = after[msg_pos + "message:".len()..].trim_start();
                let after_msg = after_msg
                    .strip_prefix("format!(")
                    .unwrap_or(after_msg)
                    .trim_start();
                if let Some(literal) = extract_string_literal(after_msg) {
                    out.push(literal);
                }
            }
            rest = &rest[pos + "SystemEvent::Toast {".len()..];
        }
        out
    }

    fn collect_rs_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(collect_rs_files(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        out
    }

    /// Finds every occurrence of `needle` in `source`, then extracts the string literal that
    /// follows it — stepping past an optional `format!(` wrapper first. Skips an occurrence
    /// entirely (rather than erroring) if what follows isn't a literal at all — `.toast(` calls
    /// that pass a variable or a helper-function call through are exactly what
    /// `extract_toast_text_helper_literals` covers separately, and everything else in this
    /// crate's `.toast(` call sites (confirmed by inspection while writing this test) is always
    /// either a bare literal or `format!("...", ...)`.
    fn extract_literals_after(source: &str, needle: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = source;
        while let Some(pos) = rest.find(needle) {
            let after = rest[pos + needle.len()..].trim_start();
            let after = after.strip_prefix("format!(").unwrap_or(after).trim_start();
            if let Some(literal) = extract_string_literal(after) {
                out.push(literal);
            }
            rest = &rest[pos + needle.len()..];
        }
        out
    }

    /// The actual returned message literals inside the body of any `fn ..._toast_text(...) ->
    /// String { ... }` — found by brace-counting from the function's own opening `{` to its
    /// matching `}`, then extracting only literals shaped like a real return value: the template
    /// argument of a `format!(...)` call, or a bare `"...".to_string()`. Deliberately *not* every
    /// string literal in the body — a match arm can bind an intermediate label (e.g. `"Album"`
    /// assigned to a local `mode` variable) that becomes part of the interpolated message rather
    /// than being a message template itself, and this test only judges the templates.
    fn extract_toast_text_helper_literals(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = source;
        let mut offset = 0;
        while let Some(fn_pos) = rest.find("fn ") {
            let after_fn = &rest[fn_pos + 3..];
            if after_fn
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .is_some_and(|name| name.ends_with("_toast_text"))
                && let Some(body_start) = after_fn.find('{')
            {
                let body = &after_fn[body_start..];
                if let Some(end) = matching_brace_end(body) {
                    let body = &body[..=end.min(body.len().saturating_sub(1))];
                    out.extend(extract_literals_after(body, "format!("));
                    out.extend(extract_literals_before(body, ".to_string()"));
                }
            }
            offset += fn_pos + 3;
            rest = &source[offset..];
        }
        out
    }

    /// Finds every occurrence of `needle`, then extracts the string literal immediately
    /// preceding it (skipping trailing whitespace) — the counterpart to `extract_literals_after`,
    /// for a bare `"literal".to_string()` return rather than a `format!(...)` one.
    fn extract_literals_before(source: &str, needle: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut search_from = 0;
        while let Some(rel_pos) = source[search_from..].find(needle) {
            let pos = search_from + rel_pos;
            let before = source[..pos].trim_end();
            if before.ends_with('"')
                && let Some(start) = find_unescaped_quote_before(before, before.len())
                && let Some(literal) = extract_string_literal(&before[start..])
            {
                out.push(literal);
            }
            search_from = pos + needle.len();
        }
        out
    }

    /// `before` ends with a closing `"` at byte index `end - 1`. Scans backward for the `"` that
    /// opens it — not preceded by a backslash. Correct for this crate's own toast literals (none
    /// contain an embedded `"` at all), not a general Rust string-literal parser.
    fn find_unescaped_quote_before(before: &str, end: usize) -> Option<usize> {
        let bytes = before.as_bytes();
        let mut i = end.checked_sub(1)?;
        while i > 0 {
            i -= 1;
            if bytes[i] == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                return Some(i);
            }
        }
        None
    }

    /// `body` must start with `{`. Returns the index of the matching `}` (relative to `body`),
    /// ignoring braces inside string/char literals (the `_toast_text` helpers this test targets
    /// have none, so a simple counter is enough — this is not a general Rust brace matcher).
    fn matching_brace_end(body: &str) -> Option<usize> {
        let mut depth = 0i32;
        for (i, c) in body.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// `s` must start with `"`. Consumes up to (and including) the closing quote, resolving the
    /// handful of escapes this codebase's own toast strings actually use — `\"`, `\\`, `\n`, and
    /// `\u{...}` (dropped entirely, not decoded to the real character: none of the audit rules
    /// below care about a literal em-dash/ellipsis's own casing or punctuation).
    fn extract_string_literal(s: &str) -> Option<String> {
        let mut chars = s.strip_prefix('"')?.chars();
        let mut result = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(result),
                '\\' => match chars.next()? {
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'u' => {
                        // `\u{XXXX}` — consume through the closing `}` and drop it.
                        for c in chars.by_ref() {
                            if c == '}' {
                                break;
                            }
                        }
                    }
                    other => result.push(other),
                },
                other => result.push(other),
            }
        }
        None
    }

    fn looks_like_url_or_token(s: &str) -> bool {
        if s.contains("://") {
            return true;
        }
        // A long run of hex-looking characters suggests a token/hash/id leaking into a message.
        let mut run = 0;
        for c in s.chars() {
            if c.is_ascii_hexdigit() {
                run += 1;
                if run >= 16 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    }

    /// `10-13`: `no_toast_message_contains_a_url_or_token`.
    #[test]
    fn no_toast_message_contains_a_url_or_token() {
        let literals = all_toast_literals();
        assert!(
            !literals.is_empty(),
            "the scanner itself found nothing — likely broken"
        );
        for (file, literal) in literals {
            assert!(
                !looks_like_url_or_token(&literal),
                "{file}: {literal:?} looks like it embeds a URL or a token"
            );
        }
    }

    /// `10-13`: `toast_messages_are_lowercase_without_trailing_period`.
    #[test]
    fn toast_messages_are_lowercase_without_trailing_period() {
        let literals = all_toast_literals();
        assert!(
            !literals.is_empty(),
            "the scanner itself found nothing — likely broken"
        );
        for (file, literal) in literals {
            if let Some(first_alpha) = literal.chars().find(|c| c.is_alphabetic()) {
                assert!(
                    first_alpha.is_lowercase(),
                    "{file}: {literal:?} must start with a lower-case letter"
                );
            }
            assert!(
                !literal.trim_end().ends_with('.'),
                "{file}: {literal:?} must not end with a trailing period"
            );
        }
    }
}
