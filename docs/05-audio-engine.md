# Audio engine

`loxia-audio` provides playback through `AudioBackend`. The production backend
uses libmpv through `libmpv2`; `MockEngine` supplies deterministic playback for
tests and no-audio operation.

## Interface

`AudioBackend` accepts `AudioCommand` values and exposes a subscription of
`AudioEvent` values. Commands include loading and preloading tracks, transport
control, seeking, volume and mute changes, equalizer changes, ReplayGain
selection, device selection, device enumeration, and shutdown.

Events report playback status, throttled position updates, audio format,
natural or explicit track end, device lists, buffering, volume state, and
errors. Stream URLs use `RedactedUrl` so diagnostic output does not expose
server credentials.

## mpv integration

`mpv::handle` translates commands to libmpv operations. `mpv::props`
centralises mpv property and option names. The engine configures audio-only
playback, gapless playlist prefetching, stream reconnect support, ReplayGain,
and HTTP request headers for Emby streams.

The backend owns its mpv interaction and communicates with the rest of the
application through channels. It is `Send`, not `Sync`.

## Equalizer and ReplayGain

The equalizer has ten bands, represented by `EQ_BANDS_HZ`, with gains clamped
to the `-12 dB` through `+12 dB` range in `0.5 dB` increments. Factory presets
come from `assets/eq_presets.toml` and are embedded in the audio crate.

The shipped factory preset names are `flat`, `darkwave_ebm`, `bass_boost`,
`vocal`, `acoustic`, `night_listening_warm`, `loudness`, and `classical`.

The libmpv backend applies the equalizer through its `af` property using an
FFmpeg `anequalizer` graph wrapped in mpv's `lavfi` bridge. ReplayGain modes
map to mpv's `album`, `track`, and `no` values. `resolve_gain` in
`loxia_core::state::player` selects tagged gain or normalization fallback for
the current track.

## Devices and gapless playback

Device labels and grouping are pure `loxia-core` model helpers. The mpv backend
enumerates and selects actual output devices. Gapless playback uses mpv's
playlist preloading and gapless options; queue reduction decides when to issue
a preload command.
