//! Lyrics, LyricLine, and the LRC parser.
//!
//! Sidecar lyric files in the wild are inconsistent, so parsing here is forgiving by design: it
//! degrades to `Unsynced` on anything it can't confidently interpret as timed lyrics, and it
//! never returns an error. Lyrics are cosmetic — a malformed file must never interrupt playback.

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Lyrics {
    Unsynced(Vec<String>),
    Synced(Vec<LyricLine>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricLine {
    pub at: Duration,
    pub text: String,
}

impl Lyrics {
    /// True when there is nothing worth showing — **including** lines that exist but are all blank.
    ///
    /// A server that answers the subtitle request with an empty body parses into exactly one empty
    /// line, which the old `v.is_empty()` counted as real content: the pane then reserved its space
    /// and drew nothing, so a track appeared to have lyrics that never showed up. Seen in the field
    /// with `kind=unsynced count=1 first=` in the logs (`docs/12-decisions.md`).
    pub fn is_empty(&self) -> bool {
        match self {
            Lyrics::Unsynced(v) => v.iter().all(|l| l.trim().is_empty()),
            Lyrics::Synced(v) => v.iter().all(|l| l.text.trim().is_empty()),
        }
    }

    /// The index of the last line whose `at <= position`, or `None` before the first line.
    /// `O(log n)` via binary search — this is called once per rendered frame. Always `None` for
    /// `Unsynced`.
    pub fn active_line(&self, position: Duration) -> Option<usize> {
        match self {
            Lyrics::Unsynced(_) => None,
            Lyrics::Synced(lines) => {
                let count = lines.partition_point(|l| l.at <= position);
                if count == 0 { None } else { Some(count - 1) }
            }
        }
    }
}

/// Known LRC metadata tag keys (case-insensitive), discarded rather than treated as lyrics.
const METADATA_KEYS: &[&str] = &["ar", "ti", "al", "by", "re", "ve", "length"];

enum Tag {
    Time(Duration),
    Offset(i64),
    Metadata,
}

/// Strips leading `[...]` tags from `line`, classifying each. Stops at the first bracket that
/// isn't a recognised tag — that bracket, and everything after it, is left in `rest`.
fn strip_leading_tags(mut line: &str) -> (Vec<Tag>, &str) {
    let mut tags = Vec::new();
    loop {
        let trimmed = line.trim_start_matches(' ');
        if !trimmed.starts_with('[') {
            break;
        }
        let after_bracket = &trimmed[1..];
        let Some(close) = after_bracket.find(']') else {
            break;
        };
        let content = &after_bracket[..close];
        match classify_tag(content) {
            Some(tag) => {
                tags.push(tag);
                line = &after_bracket[close + 1..];
            }
            None => break,
        }
    }
    (tags, line)
}

fn classify_tag(content: &str) -> Option<Tag> {
    if let Some(d) = parse_timestamp(content) {
        return Some(Tag::Time(d));
    }
    if let Some((key, value)) = content.split_once(':') {
        let key = key.trim().to_ascii_lowercase();
        if key == "offset" {
            let ms: i64 = value.trim().trim_start_matches('+').parse().unwrap_or(0);
            return Some(Tag::Offset(ms));
        }
        if METADATA_KEYS.contains(&key.as_str()) {
            return Some(Tag::Metadata);
        }
    }
    None
}

/// Parses `mm:ss`, `mm:ss.xx` (centiseconds), or `mm:ss.xxx` (milliseconds).
fn parse_timestamp(s: &str) -> Option<Duration> {
    let (minutes_str, rest) = s.split_once(':')?;
    if minutes_str.is_empty() || !minutes_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (seconds_str, frac_str) = match rest.split_once('.') {
        Some((s, f)) => (s, Some(f)),
        None => (rest, None),
    };
    if seconds_str.len() != 2 || !seconds_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let minutes: u64 = minutes_str.parse().ok()?;
    let seconds: u64 = seconds_str.parse().ok()?;
    let millis: u64 = match frac_str {
        None => 0,
        Some(f) if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) => return None,
        Some(f) => {
            let digits = &f[..f.len().min(3)];
            let value: u64 = digits.parse().ok()?;
            match digits.len() {
                1 => value * 100,
                2 => value * 10,
                _ => value,
            }
        }
    };
    Some(Duration::from_millis(
        minutes * 60_000 + seconds * 1000 + millis,
    ))
}

fn shift_by_offset(at: Duration, offset_ms: i64) -> Duration {
    // Positive offset moves lyrics earlier (LRC convention): subtract from the timestamp.
    let at_ms = at.as_millis() as i64;
    let shifted = (at_ms - offset_ms).max(0);
    Duration::from_millis(shifted as u64)
}

fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{FEFF}').unwrap_or(input)
}

fn split_lines(input: &str) -> impl Iterator<Item = &str> {
    strip_bom(input)
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
}

/// Parses an LRC file. Never errors — input that carries no recognisable timestamp anywhere
/// degrades to `Unsynced`, preserving the original lines (minus pure-metadata header lines).
pub fn parse_lrc(input: &str) -> Lyrics {
    let mut offset_ms: i64 = 0;
    let mut timed: Vec<LyricLine> = Vec::new();
    let mut untimed: Vec<String> = Vec::new();

    for raw_line in split_lines(input) {
        let (tags, rest) = strip_leading_tags(raw_line);
        let mut times = Vec::new();
        let mut saw_any_tag = false;
        for tag in tags {
            saw_any_tag = true;
            match tag {
                Tag::Time(d) => times.push(d),
                Tag::Offset(ms) => offset_ms = ms,
                Tag::Metadata => {}
            }
        }

        if !times.is_empty() {
            let text = rest.strip_prefix(' ').unwrap_or(rest).to_string();
            for at in times {
                timed.push(LyricLine {
                    at,
                    text: text.clone(),
                });
            }
        } else if saw_any_tag && rest.trim().is_empty() {
            // A pure metadata/offset line: header information, not lyric content — drop it.
        } else {
            untimed.push(raw_line.to_string());
        }
    }

    if has_usable_timing(&timed) {
        for line in &mut timed {
            line.at = shift_by_offset(line.at, offset_ms);
        }
        timed.sort_by_key(|l| l.at);
        Lyrics::Synced(timed)
    } else {
        // Same reasoning as `parse_srt`: timestamps that never differ are not timing, and honouring
        // them pins the pane to the last line. Keep whatever words were found either way.
        let mut lines: Vec<String> = timed.into_iter().map(|l| l.text).collect();
        lines.append(&mut untimed);
        Lyrics::Unsynced(lines)
    }
}

/// Plain text lyrics: always `Unsynced`, one entry per input line.
pub fn parse_plain(input: &str) -> Lyrics {
    Lyrics::Unsynced(split_lines(input).map(|l| l.to_string()).collect())
}

/// Parses an SRT cue timestamp: `HH:MM:SS,mmm`, the `,` decimal comma SRT specifies but with `.`
/// accepted too (muxers emit both). The hours field is required by the format but tolerated as
/// absent, since a stray `MM:SS,mmm` is unambiguous and refusing it would cost a whole cue's timing.
fn parse_srt_timestamp(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (whole, frac) = match s.split_once([',', '.']) {
        Some((w, f)) => (w, Some(f)),
        None => (s, None),
    };

    let mut fields: Vec<u64> = Vec::new();
    for field in whole.split(':') {
        if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        fields.push(field.parse().ok()?);
    }
    let seconds = match fields.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    };

    // A short fraction is a fraction, not a count: `,5` is half a second, `,05` is 50ms.
    let millis: u64 = match frac {
        None => 0,
        Some(f) if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) => return None,
        Some(f) => {
            let digits = &f[..f.len().min(3)];
            let value: u64 = digits.parse().ok()?;
            match digits.len() {
                1 => value * 100,
                2 => value * 10,
                _ => value,
            }
        }
    };

    Some(Duration::from_millis(seconds * 1000 + millis))
}

/// The start time of an SRT timing line (`00:00:01,000 --> 00:00:04,000`). The end time is
/// deliberately discarded: a lyric line stays on screen until the next one starts, so a cue's end
/// would only introduce gaps where the pane has nothing highlighted.
fn srt_cue_start(line: &str) -> Option<Duration> {
    parse_srt_timestamp(line.split("-->").next()?)
}

/// SRT subtitles used as a lyrics fallback. Cue numbers are dropped and each cue's **start time**
/// becomes the timestamp of every payload line in that cue, yielding [`Lyrics::Synced`] — so `.srt`
/// lyrics scroll with playback exactly like a real `.lrc` (`docs/12-decisions.md`).
///
/// A multi-line cue becomes several lines sharing one timestamp rather than one joined line: that
/// preserves the file's own line breaks, and [`Lyrics::active_line`] resolves the tie to the last
/// of them, so the whole cue is on screen with its final line highlighted.
///
/// Degrades rather than fails, at three levels: a block whose timing line is missing or
/// unparseable contributes untimed lines; if *no* block anywhere yielded a timing the result is
/// [`Lyrics::Unsynced`] over the payload text; and timings that turn out to carry no information
/// are discarded by [`has_usable_timing`].
pub fn parse_srt(input: &str) -> Lyrics {
    let mut timed: Vec<LyricLine> = Vec::new();
    let mut untimed: Vec<String> = Vec::new();
    let mut block: Vec<&str> = Vec::new();

    let flush = |block: &mut Vec<&str>, timed: &mut Vec<LyricLine>, untimed: &mut Vec<String>| {
        if block.is_empty() {
            return;
        }
        let mut lines = block.iter();
        let first_is_cue = lines
            .clone()
            .next()
            .is_some_and(|l| !l.trim().is_empty() && l.trim().bytes().all(|b| b.is_ascii_digit()));
        if first_is_cue {
            lines.next();
        }
        let at = lines
            .clone()
            .next()
            .filter(|l| l.contains("-->"))
            .and_then(|l: &&str| -> Option<Duration> { srt_cue_start(l) });
        // Consume the timing line whether or not its clock parsed — it is chrome either way, and
        // leaving an unparseable `-->` line in would show raw timecodes as lyrics.
        if lines.clone().next().is_some_and(|l| l.contains("-->")) {
            lines.next();
        }
        for payload in lines {
            if payload.is_empty() {
                continue;
            }
            match at {
                Some(at) => timed.push(LyricLine {
                    at,
                    text: payload.to_string(),
                }),
                None => untimed.push(payload.to_string()),
            }
        }
        block.clear();
    };

    for line in split_lines(input) {
        if line.trim().is_empty() {
            flush(&mut block, &mut timed, &mut untimed);
        } else {
            block.push(line);
        }
    }
    flush(&mut block, &mut timed, &mut untimed);

    if !has_usable_timing(&timed) {
        // The timestamps say nothing, so keep the words and drop the clock. `untimed` holds only
        // the strays from malformed cues, so recover the text from `timed` itself.
        let mut lines: Vec<String> = timed.into_iter().map(|l| l.text).collect();
        lines.append(&mut untimed);
        return Lyrics::Unsynced(lines);
    }
    // Stable, so the lines of one multi-line cue keep their file order under an equal timestamp.
    timed.sort_by_key(|l| l.at);
    // Untimed strays (a malformed cue among good ones) would have no position to scroll to, and
    // interleaving them at a guessed time would desync everything after. Drop them, and keep the
    // timing the rest of the file does provide.
    Lyrics::Synced(timed)
}

/// Whether a set of parsed lines carries timing worth honouring: **at least one non-zero
/// timestamp**.
///
/// Emby synthesises an `.srt` from a plain-text lyric sidecar (`txt`/`text` streams, a third of the
/// ones in this library), and every cue in it reads `00:00:00,000 --> 00:00:00,000` — real cue
/// syntax carrying no time at all. Treated as `Synced`, `active_line` finds every line at or before
/// the position from the very first frame and highlights the *last* one, so the pane jumps to the
/// bottom of the lyrics and sits there for the whole song: worse than not syncing at all. Verified
/// against the live server (`docs/12-decisions.md`).
///
/// Deliberately this narrow rather than "at least two distinct timestamps": a file whose one cue
/// starts at 0:52 is genuinely timed, and several lines sharing one real timestamp is just a
/// multi-line cue. All-zero is the case that carries no information, and the only one seen.
fn has_usable_timing(lines: &[LyricLine]) -> bool {
    lines.iter().any(|l| l.at > Duration::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server answering the subtitle request with an empty body parses into one blank line, which
    /// used to count as real content: the pane reserved space and drew nothing, so a track looked
    /// like it had lyrics that never appeared (`docs/12-decisions.md`).
    #[test]
    fn all_blank_lines_count_as_empty() {
        assert!(Lyrics::Unsynced(vec![String::new()]).is_empty());
        assert!(Lyrics::Unsynced(vec!["   ".to_string(), "\t".to_string()]).is_empty());
        assert!(
            Lyrics::Synced(vec![LyricLine {
                at: Duration::ZERO,
                text: "  ".to_string(),
            }])
            .is_empty()
        );

        assert!(!Lyrics::Unsynced(vec!["real".to_string()]).is_empty());
        assert!(
            !Lyrics::Synced(vec![LyricLine {
                at: Duration::ZERO,
                text: "real".to_string(),
            }])
            .is_empty()
        );
    }

    fn d(secs_f64: f64) -> Duration {
        Duration::from_millis((secs_f64 * 1000.0).round() as u64)
    }

    #[test]
    fn parses_centiseconds_and_milliseconds() {
        let l = parse_lrc("[01:24.00]a\n[01:24.005]b");
        match l {
            Lyrics::Synced(lines) => {
                assert_eq!(lines[0].at, d(84.00));
                assert_eq!(lines[1].at, d(84.005));
            }
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn bare_mm_ss_timestamp() {
        let l = parse_lrc("[01:24]hello");
        match l {
            Lyrics::Synced(lines) => assert_eq!(lines[0].at, d(84.0)),
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn multiple_timestamps_on_one_line_expand() {
        let l = parse_lrc("[00:12.00][01:30.00] chorus");
        match l {
            Lyrics::Synced(lines) => {
                assert_eq!(lines.len(), 2);
                assert_eq!(lines[0].text, "chorus");
                assert_eq!(lines[1].text, "chorus");
            }
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn metadata_tags_are_discarded() {
        let l = parse_lrc("[ar:Boy Harsher]\n[ti:Motion]\n[00:01.00]line one");
        match l {
            Lyrics::Synced(lines) => {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].text, "line one");
            }
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn positive_offset_shifts_earlier() {
        let l = parse_lrc("[offset:+500]\n[00:10.00]line");
        match l {
            Lyrics::Synced(lines) => assert_eq!(lines[0].at, d(9.5)),
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn negative_offset_shifts_later() {
        let l = parse_lrc("[offset:-500]\n[00:10.00]line");
        match l {
            Lyrics::Synced(lines) => assert_eq!(lines[0].at, d(10.5)),
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn untimed_lines_dropped_when_synced_lines_exist() {
        let l = parse_lrc("some untimed junk\n[00:01.00]real line");
        match l {
            Lyrics::Synced(lines) => assert_eq!(lines.len(), 1),
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn all_untimed_input_becomes_unsynced() {
        let l = parse_lrc("line one\nline two");
        assert_eq!(
            l,
            Lyrics::Unsynced(vec!["line one".to_string(), "line two".to_string()])
        );
    }

    #[test]
    fn garbage_input_never_panics_and_becomes_unsynced() {
        let l = parse_lrc("[[[not a tag at all\n[unterminated");
        assert!(matches!(l, Lyrics::Unsynced(_)));
    }

    proptest::proptest! {
        #[test]
        fn parse_lrc_never_panics(s in ".{0,200}") {
            let _ = parse_lrc(&s);
        }
    }

    #[test]
    fn handles_bom_and_crlf() {
        let l = parse_lrc("\u{FEFF}[00:01.00]hi\r\n[00:02.00]there\r\n");
        match l {
            Lyrics::Synced(lines) => {
                assert_eq!(lines.len(), 2);
                assert_eq!(lines[0].text, "hi");
                assert_eq!(lines[1].text, "there");
            }
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn lines_are_sorted_by_time() {
        let l = parse_lrc("[00:05.00]second\n[00:01.00]first");
        match l {
            Lyrics::Synced(lines) => {
                assert_eq!(lines[0].text, "first");
                assert_eq!(lines[1].text, "second");
            }
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn active_line_before_first_is_none() {
        let l = parse_lrc("[00:05.00]first");
        assert_eq!(l.active_line(Duration::from_secs(1)), None);
    }

    #[test]
    fn active_line_is_last_at_or_before_position() {
        let l = parse_lrc("[00:01.00]a\n[00:05.00]b\n[00:10.00]c");
        assert_eq!(l.active_line(Duration::from_secs(0)), None);
        assert_eq!(l.active_line(Duration::from_secs(1)), Some(0));
        assert_eq!(l.active_line(Duration::from_secs(4)), Some(0));
        assert_eq!(l.active_line(Duration::from_secs(5)), Some(1));
        assert_eq!(l.active_line(Duration::from_secs(100)), Some(2));
    }

    #[test]
    fn active_line_uses_binary_search_at_scale() {
        let mut src = String::new();
        for i in 0..100_000u32 {
            src.push_str(&format!(
                "[{:02}:{:02}.00]line {i}\n",
                (i / 60) % 100,
                i % 60
            ));
        }
        let l = parse_lrc(&src);
        let start = std::time::Instant::now();
        let _ = l.active_line(Duration::from_secs(500));
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn parse_plain_is_always_unsynced() {
        let l = parse_plain("a\nb\nc");
        assert_eq!(
            l,
            Lyrics::Unsynced(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    fn line(secs: u64, millis: u64, text: &str) -> LyricLine {
        LyricLine {
            at: Duration::from_millis(secs * 1000 + millis),
            text: text.to_string(),
        }
    }

    /// Cue numbers and the `-->` line are chrome, but the cue's **start** is the whole point: it is
    /// what lets `.srt`-fallback lyrics scroll with playback instead of sitting static.
    #[test]
    fn parse_srt_keeps_cue_start_times() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nFirst line\n\n2\n00:01:05,500 --> 00:01:08,000\nSecond line\n";
        assert_eq!(
            parse_srt(srt),
            Lyrics::Synced(vec![line(1, 0, "First line"), line(65, 500, "Second line")])
        );
    }

    /// The timestamps must be real enough to drive the pane, not merely present.
    #[test]
    fn parse_srt_output_tracks_playback_position() {
        let srt =
            "1\n00:00:01,000 --> 00:00:04,000\nFirst\n\n2\n00:00:05,000 --> 00:00:08,000\nSecond\n";
        let l = parse_srt(srt);
        assert_eq!(l.active_line(Duration::from_millis(500)), None);
        assert_eq!(l.active_line(Duration::from_secs(2)), Some(0));
        assert_eq!(l.active_line(Duration::from_secs(30)), Some(1));
    }

    /// A cue's own line breaks are preserved as separate lines sharing its start, so the pane shows
    /// the whole cue rather than one long joined row.
    #[test]
    fn parse_srt_gives_every_line_of_a_cue_the_cue_start() {
        let srt = "1\n00:00:02,000 --> 00:00:06,000\nfirst half\nsecond half\n";
        assert_eq!(
            parse_srt(srt),
            Lyrics::Synced(vec![line(2, 0, "first half"), line(2, 0, "second half")])
        );
    }

    /// Muxers emit both separators, and hour-less stamps turn up in hand-made files. Refusing
    /// either would silently cost the file its timing.
    #[test]
    fn parse_srt_accepts_decimal_point_and_omitted_hours() {
        assert_eq!(
            parse_srt("1\n00:00:01.250 --> 00:00:04.000\nDot\n"),
            Lyrics::Synced(vec![line(1, 250, "Dot")])
        );
        assert_eq!(
            parse_srt("1\n01:05,000 --> 01:08,000\nNo hours\n"),
            Lyrics::Synced(vec![line(65, 0, "No hours")])
        );
    }

    /// **Found against the live server.** Emby synthesises an `.srt` from a plain-text lyric
    /// sidecar, and every cue in it is `00:00:00,000 --> 00:00:00,000` — cue syntax with no time in
    /// it. Honoured as timing, `active_line` picks the *last* line from the very first frame and
    /// the pane sits at the bottom of the lyrics for the whole song (`docs/12-decisions.md`).
    /// Shape reproduced exactly, BOM included; the words are invented.
    #[test]
    fn an_srt_with_all_zero_timings_is_not_treated_as_timed() {
        let srt = "\u{FEFF}1\n00:00:00,000 --> 00:00:00,000\nfirst line\n\n\
                   2\n00:00:00,000 --> 00:00:00,000\nsecond line\n\n\
                   3\n00:00:00,000 --> 00:00:00,000\nthird line\n";
        let lyrics = parse_srt(srt);

        assert_eq!(
            lyrics,
            Lyrics::Unsynced(vec![
                "first line".to_string(),
                "second line".to_string(),
                "third line".to_string(),
            ]),
            "identical timestamps are not timing — keep the words, drop the clock"
        );
        assert_eq!(
            lyrics.active_line(Duration::ZERO),
            None,
            "the pane must not jump to the last line before the song has started"
        );
    }

    /// The same degenerate shape can reach `parse_lrc`, with the same consequence.
    #[test]
    fn an_lrc_with_one_repeated_timestamp_is_not_treated_as_timed() {
        // No trailing newline: `parse_lrc`'s untimed path keeps one final empty line for it,
        // which is long-standing behaviour and not what this test is about.
        let lrc = "[00:00.00]first line\n[00:00.00]second line";
        assert_eq!(
            parse_lrc(lrc),
            Lyrics::Unsynced(vec!["first line".to_string(), "second line".to_string()])
        );
    }

    /// The guard must stay narrow: a single genuinely-timed cue is still timed, and a cue that
    /// merely *starts* at zero alongside later ones is normal.
    #[test]
    fn a_real_timestamp_anywhere_keeps_the_file_synced() {
        let srt =
            "1\n00:00:00,000 --> 00:00:04,000\nfirst\n\n2\n00:00:09,000 --> 00:00:12,000\nsecond\n";
        let lyrics = parse_srt(srt);
        assert_eq!(
            lyrics,
            Lyrics::Synced(vec![line(0, 0, "first"), line(9, 0, "second")])
        );
        assert_eq!(lyrics.active_line(Duration::from_secs(10)), Some(1));
    }

    /// Nothing timed anywhere: identical to the behaviour before timings were parsed at all.
    #[test]
    fn parse_srt_falls_back_on_malformed_block() {
        let l = parse_srt("just some text\nmore text");
        assert_eq!(
            l,
            Lyrics::Unsynced(vec!["just some text".to_string(), "more text".to_string()])
        );
    }

    /// One bad cue must not cost the file the timing every other cue does supply — and the bad
    /// cue's raw timecode line must not leak into the lyrics as if it were a lyric.
    #[test]
    fn parse_srt_keeps_timings_despite_one_unparseable_cue() {
        let srt = "1\nnot:a:time --> whatever\nStray\n\n2\n00:00:09,000 --> 00:00:12,000\nGood\n";
        assert_eq!(parse_srt(srt), Lyrics::Synced(vec![line(9, 0, "Good")]));
    }

    proptest::proptest! {
        /// Lyrics are cosmetic: no input may panic, and a `Synced` result is always sorted so
        /// `active_line`'s binary search stays correct.
        #[test]
        fn parse_srt_never_panics_and_stays_sorted(s in ".{0,300}") {
            if let Lyrics::Synced(lines) = parse_srt(&s) {
                proptest::prop_assert!(lines.windows(2).all(|w| w[0].at <= w[1].at));
            }
        }
    }

    #[test]
    fn is_empty_true_for_empty_variants() {
        assert!(Lyrics::Unsynced(vec![]).is_empty());
        assert!(Lyrics::Synced(vec![]).is_empty());
        assert!(!Lyrics::Unsynced(vec!["x".to_string()]).is_empty());
    }
}
