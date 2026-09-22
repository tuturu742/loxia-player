# Decisions

This document records implementation decisions that materially affect documented behaviour.

## 1. Core remains I/O-free

`loxia-core` contains domain logic only. Runtime workers own terminal, filesystem, network, audio,
and operating-system integration.

## 2. Stream URLs are redacted values

Stream URLs and access tokens are represented with redaction-aware types where diagnostics can
reach them. Logs and error displays use generic messages rather than credentials or complete URLs.

## 3. The audio backend is channel-oriented

`AudioBackend` is `Send`, not `Sync`. The libmpv implementation owns its playback thread and
communicates through channels, which matches the runtime's single-owner audio model.

## 4. EQ resets the mpv filter property

The equalizer uses a `lavfi`-wrapped `anequalizer` graph assigned to mpv's `af` property.
The runtime updates the complete graph because `af-command` does not reliably route the
`anequalizer` runtime command through the lavfi bridge. Updating `af` does not restart playback on
the supported mpv behaviour.

## 5. Platform text uses runtime target detection

Audio-library installation hints use `std::env::consts::OS`. This produces target-specific text
without spreading conditional compilation through the audio crate.

## 6. Direct terminal dependencies are prohibited

The workspace accesses crossterm through ratatui. It does not declare `crossterm` directly.
Likewise, it uses `jiff` rather than declaring `time` directly.

## 7. Audio names Tokio for channel types

`loxia-audio` directly depends on Tokio with only its `sync` feature because its public backend
subscription type is Tokio's MPSC receiver. It does not own or start a Tokio runtime.

## 8. Fixture scanning targets credentials, not identifiers

Fixture scanning checks token-bearing fields and token query parameters. It does not reject generic
hex identifiers because Emby responses legitimately contain IDs, ETags, and image tags with that
shape.

## 9. Behaviour-affecting documentation corrections

| Date | Correction | Reason |
|---|---|---|
| 2026-09-22 | The reference documentation describes the implemented workspace and records unimplemented work only in `ROADMAP.md`. | The former blueprint and task-library framing did not describe current behaviour. |
