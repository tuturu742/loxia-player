# Audio engine

`loxia-audio` provides playback behind the `AudioBackend` trait. The application communicates with
this trait through `AudioCommand` and receives asynchronous `AudioEvent` values.

## Backends

`MpvEngine` is the production backend and owns libmpv integration. `MockEngine` is deterministic
and supports tests without an audio device or libmpv installation. Both implement `AudioBackend`.

The backend accepts commands to load, preload, play, pause, stop, seek, change volume or mute
state, configure ReplayGain and equalizer settings, enumerate devices, select a device, and shut
down. It reports status, positions, formats, buffering, track endings, device lists, volume state,
and errors.

## libmpv integration

The `mpv` module centralises libmpv option and property names in `mpv::props`. `mpv::handle`
translates commands and receives libmpv events. `mpv::filters` applies equalizer filters. The
backend enables mpv playlist prefetching for next-track preloading and uses the normal system audio
output unless a test explicitly selects the null output.

The binary reports a platform-specific installation hint when libmpv is unavailable.

## Equalizer and ReplayGain

The equalizer uses ten ISO bands defined by `EQ_BANDS_HZ`. Factory presets are embedded from
`assets/eq_presets.toml`: `flat`, `darkwave_ebm`, `bass_boost`, `vocal`, `acoustic`,
`night_listening_warm`, `loudness`, and `classical`.

The production filter path uses mpv's `af` property with a `lavfi`-wrapped `anequalizer` graph.
ReplayGain maps the configured `Album`, `Track`, and `Off` modes to mpv properties and resolves
embedded gain metadata before normalisation fallback.

## Devices

Device labels and grouping are pure `loxia-core` model functions. The audio backend asks mpv for
available devices and applies the selected device identifier.

See [`12-decisions.md`](12-decisions.md) for implementation choices that differ from an earlier
design assumption.
