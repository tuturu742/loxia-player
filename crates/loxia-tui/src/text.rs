//! Grapheme-safe text measurement and truncation (`docs/07-ui-spec.md` §13).
//!
//! Every public function here sanitises its input first — stripping control characters and ANSI
//! escape sequences — since track titles, album names, and every other display string ultimately
//! come from the server. An embedded escape sequence must never be able to move the cursor or
//! repaint the screen just because a widget printed a track title.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The default ellipsis. `docs/07-ui-spec.md` §13 also calls for an ASCII `"..."` variant when a
/// theme is `ascii_only`, but neither `ellipsize` nor `ellipsize_start` takes a theme/flag
/// parameter in this task's given signature — see `docs/12-decisions.md`. `ellipsize_with` (not
/// part of that signature list, added alongside it) is what an `ascii_only`-aware caller uses
/// instead, passing `"..."`.
const ELLIPSIS: &str = "…";

fn needs_sanitizing(s: &str) -> bool {
    s.chars().any(|c| c.is_control())
}

/// Strips every control character (including bare `ESC`) and any full ANSI CSI escape sequence
/// (`ESC '[' <parameter/intermediate bytes> <final byte>`) from `s`.
fn sanitize_owned(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c2 in chars.by_ref() {
                    if c2.is_ascii_alphabetic() || c2 == '~' {
                        break;
                    }
                }
            }
            continue;
        }
        if c.is_control() {
            continue;
        }
        out.push(c);
    }
    out
}

fn maybe_sanitize(s: &str) -> Cow<'_, str> {
    if needs_sanitizing(s) {
        Cow::Owned(sanitize_owned(s))
    } else {
        Cow::Borrowed(s)
    }
}

fn width_raw(s: &str) -> usize {
    s.width()
}

/// Pins a glyph to its **text** presentation, so the terminal draws it in one cell.
///
/// The chrome used to draw several "text-default but emoji-capable" glyphs (`♥`, `☺`, `⚙`, `▶`,
/// `⏸`, `⏹`, `⚠`, `⏱`): `unicode-width` measures them as one cell, and a terminal with an emoji
/// font happily draws them as two. Layout is computed from our width, so the label beside such a
/// glyph ends up a column out — reported as the Favourites heart and the Artists face looking
/// off-centre. U+FE0E is the variation selector that says "text, not emoji".
///
/// **The selector is a backstop, not the fix.** A terminal is free to ignore U+FE0E, and the ones
/// that do kept drawing those glyphs two cells wide — the same report came back a second time
/// after this function was added. The real fix is not to use an emoji-capable glyph in the chrome
/// at all: every one of them has been swapped for a look-alike that has no emoji presentation
/// (`♥`→`♡`, `☺`→`☻`, `⚙`→`⛭`, `▶`→`▸`, `⏸`→`‖`, `⏹`→`■`, `⚠`→`△`, `⏱`→`⧗`), and
/// [`CHROME_GLYPHS`] pins that. This stays so a glyph added later without checking is still
/// handled as well as it can be (`docs/12-decisions.md`).
///
/// Detected rather than hardcoded — if the emoji form would be wider, the glyph is at risk — so a
/// glyph added later is handled without anyone having to remember this exists. A glyph with no
/// emoji presentation is returned untouched, since appending a selector it doesn't participate in
/// only risks a stray box.
pub fn narrow_glyph(c: char) -> String {
    // ASCII is never drawn as emoji on its own: `#` and the digits only become one as part of a
    // *keycap sequence* (selector plus U+20E3), which nothing here builds. Pinning them would add a
    // codepoint to ordinary text for no gain.
    if c.is_ascii() {
        return c.to_string();
    }
    let bare = c.to_string();
    let emoji = format!("{c}\u{FE0F}");
    if width_raw(&emoji) > width_raw(&bare) {
        format!("{c}\u{FE0E}")
    } else {
        bare
    }
}

/// Every glyph the chrome draws in a fixed-width slot: sidebar tabs, player-bar status and
/// availability, column row markers, header badges, cursors. The list exists so
/// `no_chrome_glyph_is_emoji_capable` can hold the whole set to one rule — add a glyph here when
/// you add one to the UI, and the test tells you immediately if it is one a terminal might draw
/// two cells wide.
pub const CHROME_GLYPHS: &str = "♪♡⚲≡☻◉◫#⌂⛭⏻▸‖■⋯◌◐↓△⇄⧗";

/// Display width in terminal cells — never `str::len()` (bytes) or `chars().count()`, both of
/// which are wrong for CJK or emoji.
pub fn width(s: &str) -> usize {
    width_raw(&maybe_sanitize(s))
}

/// A grapheme-safe prefix of `s` no wider than `max` cells. A grapheme that would straddle the
/// limit is dropped entirely rather than half-rendered.
fn truncate_owned(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let mut used = 0;
    let mut end = 0;
    for g in s.graphemes(true) {
        let w = width_raw(g);
        if used + w > max {
            break;
        }
        used += w;
        end += g.len();
    }
    s[..end].to_string()
}

/// The mirror of [`truncate_owned`]: a grapheme-safe *suffix* of `s`, for `ellipsize_start`.
fn truncate_keep_suffix_owned(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let mut used = 0;
    let mut start = s.len();
    for g in s.graphemes(true).rev() {
        let w = width_raw(g);
        if used + w > max {
            break;
        }
        used += w;
        start -= g.len();
    }
    s[start..].to_string()
}

/// Hard cut at a grapheme boundary — never splits a multi-byte character or separates a
/// combining mark from its base.
pub fn truncate(s: &str, max: usize) -> Cow<'_, str> {
    if max == 0 {
        return Cow::Borrowed("");
    }
    let sanitized = maybe_sanitize(s);
    if width_raw(&sanitized) <= max {
        return sanitized;
    }
    Cow::Owned(truncate_owned(&sanitized, max))
}

/// Cut and append an ellipsis, reserving its width from `max`. Below `max = 2` there is no room
/// to reserve an ellipsis and still show anything meaningful, so this degrades to a plain
/// [`truncate`].
pub fn ellipsize(s: &str, max: usize) -> Cow<'_, str> {
    ellipsize_with(s, max, ELLIPSIS)
}

/// As [`ellipsize`], but with a caller-supplied ellipsis string — how an `ascii_only` theme gets
/// `"..."` instead of `"…"` (see this module's doc comment).
pub fn ellipsize_with<'a>(s: &'a str, max: usize, ellipsis: &str) -> Cow<'a, str> {
    let sanitized = maybe_sanitize(s);
    if width_raw(&sanitized) <= max {
        return sanitized;
    }
    if max < 2 {
        return Cow::Owned(truncate_owned(&sanitized, max));
    }
    let ellipsis_w = width_raw(ellipsis);
    let budget = max.saturating_sub(ellipsis_w);
    let head = truncate_owned(&sanitized, budget);
    Cow::Owned(format!("{head}{ellipsis}"))
}

/// Like [`ellipsize`] but keeps the *end* of `s` and prepends the ellipsis — for paths:
/// `"…/Darkwave/"`.
pub fn ellipsize_start(s: &str, max: usize) -> Cow<'_, str> {
    ellipsize_start_with(s, max, ELLIPSIS)
}

/// As [`ellipsize_start`], with a caller-supplied ellipsis string.
pub fn ellipsize_start_with<'a>(s: &'a str, max: usize, ellipsis: &str) -> Cow<'a, str> {
    let sanitized = maybe_sanitize(s);
    if width_raw(&sanitized) <= max {
        return sanitized;
    }
    if max < 2 {
        return Cow::Owned(truncate_keep_suffix_owned(&sanitized, max));
    }
    let ellipsis_w = width_raw(ellipsis);
    let budget = max.saturating_sub(ellipsis_w);
    let tail = truncate_keep_suffix_owned(&sanitized, budget);
    Cow::Owned(format!("{ellipsis}{tail}"))
}

/// Pads `s` with spaces to reach exactly `w` cells. Never truncates — a string already `>= w`
/// wide is returned unchanged (sanitised).
pub fn pad_to(s: &str, w: usize) -> String {
    let sanitized = maybe_sanitize(s);
    let current = width_raw(&sanitized);
    if current >= w {
        return sanitized.into_owned();
    }
    let mut out = sanitized.into_owned();
    out.push_str(&" ".repeat(w - current));
    out
}

/// Distributes `total` cells across `items.len()` columns with `gap` cells between neighbours,
/// giving any remainder to the leftmost columns so the layout doesn't jitter as widths change by
/// a cell or two. Only `items.len()` is used — the strings themselves aren't measured, since this
/// is a column-count-based split, not a content-based one.
pub fn fit_columns(items: &[&str], total: usize, gap: usize) -> Vec<usize> {
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    let gaps_total = gap.saturating_mul(n - 1);
    let available = total.saturating_sub(gaps_total);
    let base = available / n;
    let remainder = available % n;
    (0..n).map(|i| base + usize::from(i < remainder)).collect()
}

/// Grapheme-safe greedy word wrap to `w` cells per line. A single word wider than `w` is hard-cut
/// across as many lines as needed rather than overflowing one line.
pub fn wrap(s: &str, w: usize) -> Vec<String> {
    let sanitized = maybe_sanitize(s);
    if w == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in sanitized.split_whitespace() {
        let word_width = width_raw(word);

        if word_width > w {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                current_width = 0;
            }
            let mut rest = word;
            while !rest.is_empty() {
                let piece = truncate_owned(rest, w);
                let piece_len = piece.len();
                rest = &rest[piece_len..];
                lines.push(piece);
            }
            continue;
        }

        let needed = if current.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width
        };

        if needed > w {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_width = word_width;
        } else {
            if !current.is_empty() {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph the chrome draws in a fixed-width slot must measure one cell.
    #[test]
    fn chrome_glyphs_are_one_cell() {
        for c in CHROME_GLYPHS.chars() {
            assert_eq!(
                width(&narrow_glyph(c)),
                1,
                "U+{:04X} measures {} cells",
                c as u32,
                width(&c.to_string())
            );
        }
    }

    /// The real guarantee, stronger than the one above: **no chrome glyph may have an emoji
    /// presentation at all.**
    ///
    /// Measuring one cell is not enough, because it is our measurement, not the terminal's. An
    /// emoji-capable glyph is drawn two cells wide by any terminal with an emoji font, and the
    /// layout — computed from `unicode-width` — then puts everything beside it a column out. U+FE0E
    /// is supposed to prevent that, but a terminal may ignore it, and the "off-centre heart" report
    /// came back after the selector was already being emitted. Choosing glyphs the question never
    /// arises for is the only fix that does not depend on the terminal (`docs/12-decisions.md`).
    #[test]
    fn no_chrome_glyph_is_emoji_capable() {
        for c in CHROME_GLYPHS.chars() {
            if c.is_ascii() {
                continue;
            }
            let emoji = format!("{c}\u{FE0F}");
            assert_eq!(
                width(&emoji),
                width(&c.to_string()),
                "U+{:04X} ({c}) has an emoji presentation and must not be used in the chrome — \
                 pick a look-alike with none rather than relying on U+FE0E",
                c as u32
            );
        }
    }

    /// The selector is only added where it does something: a glyph with no emoji presentation is
    /// returned untouched, so no terminal is asked to interpret a selector it doesn't participate
    /// in.
    #[test]
    fn only_emoji_capable_glyphs_get_a_selector() {
        // Deliberately glyphs the chrome no longer uses — that is the point of `CHROME_GLYPHS`.
        for c in ['\u{2665}', '\u{263A}', '\u{2699}', '\u{25B6}', '\u{26A0}'] {
            assert!(
                narrow_glyph(c).contains('\u{FE0E}'),
                "U+{:04X} has an emoji form and must be pinned",
                c as u32
            );
        }
        for c in ['\u{266A}', '\u{263B}', '\u{2261}', '\u{2302}', '#'] {
            assert_eq!(
                narrow_glyph(c),
                c.to_string(),
                "U+{:04X} has no emoji form and must be left alone",
                c as u32
            );
        }
    }

    use proptest::prelude::*;

    #[test]
    fn width_counts_cells_not_bytes() {
        assert_eq!(width("日本語"), 6);
        assert_eq!(width("café"), 4);
        assert_eq!(width("👍"), 2);
    }

    proptest! {
        #[test]
        fn truncate_never_splits_a_grapheme(s in ".{0,40}", max in 0usize..20) {
            let result = truncate(&s, max);
            prop_assert!(std::str::from_utf8(result.as_bytes()).is_ok());
            prop_assert!(width(&result) <= max);
        }
    }

    #[test]
    fn truncate_drops_straddling_wide_char() {
        assert_eq!(truncate("日本", 1), "");
    }

    #[test]
    fn ellipsize_reserves_ellipsis_width() {
        let result = ellipsize("a very long track title indeed", 10);
        assert!(width(&result) <= 10);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn ellipsize_with_tiny_max_degrades() {
        assert_eq!(ellipsize("hello", 1), truncate("hello", 1));
        assert_eq!(ellipsize("hello", 0), truncate("hello", 0));
        assert!(!ellipsize("hello", 1).contains('…'));
    }

    #[test]
    fn zero_max_returns_empty() {
        assert_eq!(truncate("hello", 0), "");
        assert_eq!(ellipsize("hello", 0), "");
        assert_eq!(ellipsize_start("hello", 0), "");
    }

    #[test]
    fn combining_marks_stay_with_base() {
        // "e" + combining acute accent (U+0301) — one grapheme cluster.
        let s = "e\u{0301}bc";
        let result = truncate(s, 1);
        assert_eq!(result, "e\u{0301}");
    }

    #[test]
    fn control_chars_are_stripped() {
        assert_eq!(sanitize_owned("a\u{7}b\u{8}c"), "abc");
        assert_eq!(width("a\u{7}b"), 2);
    }

    #[test]
    fn ansi_escape_is_stripped() {
        assert_eq!(sanitize_owned("\x1b[2Jtitle"), "title");
        assert_eq!(truncate("\x1b[2Jtitle", 20), "title");
    }

    #[test]
    fn wrap_is_grapheme_safe() {
        let lines = wrap("日本語 test wrapping behaviour here", 6);
        for line in &lines {
            assert!(width(line) <= 6, "{line:?} exceeds width 6");
            assert!(std::str::from_utf8(line.as_bytes()).is_ok());
        }
    }

    #[test]
    fn fit_columns_sums_to_total() {
        let items = ["a", "bb", "ccc"];
        let widths = fit_columns(&items, 30, 1);
        assert_eq!(widths.len(), 3);
        let sum: usize = widths.iter().sum::<usize>() + 2; // + 2 gaps between 3 columns
        assert_eq!(sum, 30);
    }

    #[test]
    fn fit_columns_gives_remainder_to_leftmost() {
        let items = ["a", "b", "c"];
        let widths = fit_columns(&items, 10, 0);
        assert_eq!(widths, vec![4, 3, 3]);
    }

    #[test]
    fn fit_columns_empty_items_is_empty() {
        let items: [&str; 0] = [];
        assert_eq!(fit_columns(&items, 10, 1), Vec::<usize>::new());
    }
}
