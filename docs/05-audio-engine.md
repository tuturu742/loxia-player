# Audio engine

`loxia-audio` supplies the playback boundary through `AudioBackend`. The production backend is
libmpv-backed; `MockEngine` provides deterministic playback for tests and no-audio operation.

## Commands and events

`AudioCommand` covers loading and preloading media, transport controls, seeking, volume and mute,
equalizer and ReplayGain settings, device selection, enumeration, and shutdown. `AudioEvent`
reports status, throttled position updates, media format, natural track completion, devices,
buffering, volume, and errors.

Audio stream URLs use `RedactedUrl`. The backend accepts custom request headers without exposing
credentials through debug output.

## mpv integration

The mpv backend owns the libmpv handle and translates commands to mpv options and properties.
Playback is configured for audio-only operation, playlist prefetching, and gapless audio. Position
events are throttled to four updates per second.

The backend uses mpv's audio-device list for enumeration and `audio-device` for switching devices.
Platform-specific installation guidance is provided when libmpv is unavailable.

## Equalizer and ReplayGain

The equalizer uses ten ISO-band center frequencies:

`31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000` Hz.

Gain values are limited to ±12 dB in 0.5 dB increments. The bundled factory presets are `flat`,
`darkwave_ebm`, `bass_boost`, `vocal`, `acoustic`, `night_listening_warm`, `loudness`, and
`classical`.

The engine installs the EQ through mpv's `af` property using a `lavfi`-wrapped `anequalizer`
filter graph. ReplayGain maps the configured `Album`, `Track`, and `Off` modes to mpv's
`album`, `track`, and `no` values.

## Testing

Most audio tests use `MockEngine`. Tests requiring a real libmpv instance are gated behind the
`mpv-tests` feature and are not required by ordinary workspace test runs.

See [`12-decisions.md`](12-decisions.md) for the documented mpv behaviour decisions.
