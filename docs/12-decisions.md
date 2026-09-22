# Decisions

This document records decisions that affect documented behaviour, especially where implementation
details differ from earlier assumptions.

## 1. Core remains I/O-free

`loxia-core` contains domain logic only. Runtime workers perform terminal, network, audio, and
filesystem I/O.

## 2. Key hints resolve through the keymap

Views render bindings through `KeyMap::hint_for(ActionId)` rather than embedding literal key text.
This keeps help and contextual hints correct after user remapping.

## 3. Stream URLs are redacted

Audio commands carry `RedactedUrl` rather than plain stream URL strings. Debug and display output
therefore avoids exposing server tokens.

## 4. Equalizer updates reset the mpv filter property

The equalizer installs the `lavfi`-wrapped `anequalizer` graph through mpv's `af` property. The
runtime command route for mutating a `lavfi` subgraph is not reliable across the supported mpv and
FFmpeg combinations, while resetting `af` preserves playback on the tested backend.

## 5. Audio-device grouping is domain data

Audio-device labels and grouping live in `loxia-core` because both the audio backend and terminal
UI require them.

## 6. Platform install hints use runtime target identification

The libmpv installation hint matches `std::env::consts::OS`. This avoids spreading target-specific
conditional compilation through the audio crate.

## 7. Real libmpv tests are optional

Tests requiring an installed libmpv run only with the `mpv-tests` feature. The normal workspace
suite remains runnable without audio hardware or libmpv.

## 8. Fixture scanning targets credential-bearing fields

CI scans fields and query parameters that can contain credentials. It does not reject arbitrary
long hexadecimal identifiers because real Emby fixture payloads legitimately contain such IDs.

## 9. Behaviour documentation corrections

| Date | Correction | Reason |
|---|---|---|
| 2026-09-22 | Replaced blueprint framing with present-tense reference documentation and moved unimplemented packaging automation to `ROADMAP.md`. | The documentation now describes the checked-in system rather than an implementation plan. |
