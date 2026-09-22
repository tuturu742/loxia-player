# Audio engine

`loxia-audio` exposes the `AudioBackend` abstraction and provides a libmpv-backed implementation
and a deterministic `MockEngine`.

## Backend boundary

`AudioCommand` carries commands such as loading, preloading, seeking, playback control, volume and
mute changes, device selection, equalizer updates, ReplayGain changes, and shutdown. `AudioEvent`
returns status, position, format, track-end, buffering, device, volume, and error information.

The backend trait uses command delivery and a subscription receiver so code above the backend does
not depend on libmpv types.

## libmpv implementation

The `mpv` module owns property names, handle operations, filter handling, and event translation.
The engine configures libmpv for audio playback and maps its state into core player values.
Position updates are throttled before entering the application loop.

Audio diagnostics provide platform-specific installation hints when libmpv is unavailable without
placing platform-specific implementation branches throughout the application.

## Equalizer and ReplayGain

The equalizer is a ten-band curve using the frequencies in `EQ_BANDS_HZ`. Factory presets are
embedded from `assets/eq_presets.toml`. The mpv implementation applies its `lavfi`-wrapped
`anequalizer` chain through the `af` property.

ReplayGain modes map to mpv's `replaygain` property. Core state records the gain choice so the UI
can explain whether tags, normalization, or no gain source applies.

## Devices and gapless playback

Audio-device labels and grouping are shared model helpers. The audio engine enumerates mpv devices
and applies selected device identifiers.

Preloading appends the next item to mpv's playlist. Together with mpv gapless and playlist-prefetch
options, this supports gapless transitions when queue logic requests a preload.
