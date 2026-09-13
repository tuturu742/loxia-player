# 01-08 · Lyrics model and LRC parser

**Phase:** 01 — Config · **Agent:** A · **Size:** S
**Prerequisites:** `01-07`
**Reference:** `docs/02-data-model.md` §2, `docs/03-emby-api.md` §7

## Goal
Model lyrics and parse LRC. Sidecar files in the wild are inconsistent, so the parser must be
forgiving: it degrades, and it never errors.

## Files
- `crates/loxia-core/src/model/lyrics.rs`

## Specification

```
pub enum Lyrics { Unsynced(Vec<String>), Synced(Vec<LyricLine>) }
pub struct LyricLine { pub at: Duration, pub text: String }

pub fn parse_lrc(input: &str) -> Lyrics;
pub fn parse_plain(input: &str) -> Lyrics;      // always Unsynced
pub fn parse_srt(input: &str) -> Lyrics;        // Unsynced; drops cue numbers and timings

impl Lyrics {
    pub fn is_empty(&self) -> bool;
    pub fn active_line(&self, position: Duration) -> Option<usize>;
}
```

**`parse_lrc` requirements:**
- Timestamps `[mm:ss.xx]`, `[mm:ss.xxx]`, and `[mm:ss]`. Two-digit fractions are **centiseconds**,
  three-digit are milliseconds — getting this wrong desynchronises everything by up to a second.
- Multiple timestamps on one line (`[00:12.00][01:30.00] chorus`) produce one `LyricLine` per stamp.
- Metadata tags `[ar:]`, `[ti:]`, `[al:]`, `[by:]`, `[re:]`, `[ve:]`, `[length:]` are discarded.
- `[offset:±ms]` shifts every timestamp; a positive offset moves lyrics **earlier**, per the LRC
  convention.
- Lines with no timestamp are dropped when at least one timestamped line exists; when **none** does,
  the whole input is returned as `Unsynced`.
- Output is sorted by `at`, stably.
- Empty text after a timestamp is kept — it renders as a blank line and is how gaps are expressed.
- Leading BOM and CRLF line endings are handled.

**`active_line`** returns the index of the last line whose `at <= position`, or `None` before the
first. It is called once per frame, so it must be `O(log n)` via binary search, not a linear scan.
For `Unsynced` it always returns `None`.

**The parser never returns an error.** Malformed input degrades to `Unsynced` with the raw lines.

## Acceptance
- `parses_centiseconds_and_milliseconds` — `[01:24.00]` is 84.00 s; `[01:24.005]` is 84.005 s.
- `bare_mm_ss_timestamp`
- `multiple_timestamps_on_one_line_expand`
- `metadata_tags_are_discarded`
- `positive_offset_shifts_earlier`
- `untimed_lines_dropped_when_synced_lines_exist`
- `all_untimed_input_becomes_unsynced`
- `garbage_input_becomes_unsynced_never_errors` (proptest over arbitrary strings — the property is
  that `parse_lrc` returns without panicking for every input)
- `handles_bom_and_crlf`
- `lines_are_sorted_by_time`
- `active_line_before_first_is_none`
- `active_line_is_last_at_or_before_position`
- `active_line_uses_binary_search` — 100k lines resolve in well under a millisecond.

## Done when
The global DoD in `tasks/README.md` is satisfied.
