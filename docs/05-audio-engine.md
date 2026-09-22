# Audio engine

`loxia-audio` provides playback behind the `AudioBackend` trait.

## Backend contract

`AudioBackend` accepts `AudioCommand` values and exposes an unbounded receiver of `AudioEvent`
values. The production backend is `mpv::handle::MpvEngine`; `MockEngine` provides deterministic
test control without libmpv or an audio device.

Commands cover loading, preloading, transport controls, seeking, volume and mute, equalizer state,
ReplayGain mode, output-device selection, enumeration, and shutdown. Events report status,
position, format, track endings, devices, buffering, volume, and errors.

## libmpv integration

The mpv backend owns its mpv handle and translates the backend contract to mpv options, properties,
commands, and callbacks. It enables gapless playlist behaviour and prefetching, receives position
updates at a bounded rate, and reports playback state through `AudioEvent`.

Stream URLs use `RedactedUrl` so debug output does not expose authentication query parameters.
Headers are passed at the load boundary rather than retained in application state.

## Equalizer and ReplayGain

The equalizer uses ten ISO-band frequencies exposed by `EQ_BANDS_HZ`. `eq` builds an FFmpeg
`anequalizer` graph wrapped by mpv's `lavfi` bridge. Gain values clamp to the supported ±12 dB
range in 0.5 dB increments. Factory presets come from `assets/eq_presets.toml`.

ReplayGain modes map to mpv's `replaygain` values. Gain selection is shared with
`loxia-core::state::player`, allowing reducers and the audio backend to agree on the applied gain.

## Devices and testing

Audio devices use `AudioDevice` domain values and are grouped through helpers in `loxia-core`.
Real-mpv tests are gated behind the `mpv-tests` feature; normal workspace tests use the mock backend.
