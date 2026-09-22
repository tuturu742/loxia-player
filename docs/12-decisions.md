# Decisions

This document records implementation choices that affect architecture,
behaviour, dependency boundaries, or documentation. Source code and tests show
the exact current mechanics; these entries preserve the reason for choices that
are not obvious from an API alone.

## 1. Pure core boundary

`loxia-core` contains no I/O. Network, filesystem, terminal, audio, and
platform integration remain in adapter crates or `loxia-player`. This keeps
state transitions deterministic and independently testable.

## 2. Effects mediate I/O

Reducers emit effects instead of performing side effects. Runtime workers
execute those effects and send events back through dispatch. The arrangement
keeps input, rendering, and worker completion on one application state path.

## 3. Sensitive values are redacted

Authentication tokens and stream URLs do not appear in ordinary diagnostic
output. Stream URLs use `RedactedUrl`, and fixtures contain no live
credentials or private addresses.

## 4. mpv owns playback mechanics

libmpv provides decoding, output-device interaction, playlist prefetching, and
playback observation. `loxia-audio` exposes a narrow command/event abstraction
so the rest of the workspace does not depend on libmpv APIs.

## 5. Equalizer updates reset the filter property

The production backend applies the `lavfi`-wrapped `anequalizer` graph through
mpv's `af` property. Real mpv testing showed that the documented
`af-command` route does not reliably reach the wrapped filter, while resetting
the property does not restart the track on the tested backend.

## 6. Platform labels are runtime data

Audio library installation hints select text through `std::env::consts::OS`
rather than scattered target-conditional compilation. This preserves the
project's limited use of platform `cfg` attributes.

## 7. Direct dependency policy is enforced in CI

`crossterm` and `time` remain transitive dependencies only. The workspace uses
ratatui's crossterm re-export and `jiff` for time-oriented data. CI checks
crate manifests because a graph-wide denial cannot express this distinction.

## 8. Fixtures are realistic but scrubbed

Emby fixtures retain realistic IDs and response shapes. CI scans only
token-bearing fields and private addresses, avoiding false positives from
ordinary Emby identifiers.

## 9. Documentation corrections

| Date | Correction | Reason |
|---|---|---|
| 2026-09-22 | The reference set identifies `loxia-player` as the runtime executable crate and `loxia-tui` as the presentation crate. | This matches the workspace crate layout and avoids describing the TUI crate as the application process boundary. |
