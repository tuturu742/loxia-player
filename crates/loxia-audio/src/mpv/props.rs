//! Typed mpv property/option name constants (`docs/05-audio-engine.md` §3). No string literal for
//! an mpv property or option may appear anywhere else in this crate — a typo in one is a silent
//! no-op that is very hard to find; `property_names_are_centralised` (in `handle.rs`) greps the
//! rest of the crate to enforce this.

// Initialisation options (`Mpv::with_initializer`).
pub const OPT_VIDEO: &str = "video";
pub const OPT_AUDIO_DISPLAY: &str = "audio-display";
pub const OPT_TERMINAL: &str = "terminal";
pub const OPT_MSG_LEVEL: &str = "msg-level";
pub const OPT_IDLE: &str = "idle";
pub const OPT_KEEP_OPEN: &str = "keep-open";
pub const OPT_GAPLESS_AUDIO: &str = "gapless-audio";
pub const OPT_PREFETCH_PLAYLIST: &str = "prefetch-playlist";
pub const OPT_CACHE: &str = "cache";
pub const OPT_CACHE_SECS: &str = "cache-secs";
pub const OPT_AUDIO_CLIENT_NAME: &str = "audio-client-name";
pub const OPT_REPLAYGAIN: &str = "replaygain";
pub const OPT_REPLAYGAIN_PREAMP: &str = "replaygain-preamp";
pub const OPT_REPLAYGAIN_CLIP: &str = "replaygain-clip";
pub const OPT_VOLUME_MAX: &str = "volume-max";
pub const OPT_USER_AGENT: &str = "user-agent";
/// mpv's youtube-dl/yt-dlp hook. Off: this plays files from a media server, never a video site, and
/// leaving it on means any URL mpv cannot parse — an HTML error page from a proxy, say — triggers a
/// youtube-dl subprocess whose failure buries the real cause under `ytdl_hook` noise
/// (`docs/12-decisions.md`).
pub const OPT_YTDL: &str = "ytdl";
/// Options passed through to libavformat's stream (protocol) layer — used to turn on ffmpeg's HTTP
/// auto-reconnect so a long pause (which lets the server drop the idle connection) can resume
/// instead of dying (`docs/05-audio-engine.md` §3, `docs/12-decisions.md`).
pub const OPT_STREAM_LAVF_O: &str = "stream-lavf-o";
/// Never set by production code — mpv auto-selects the real output. Only the `mpv-tests` suite
/// forces this, to `"null"`, for deterministic hardware-independent playback timing.
pub const OPT_AO: &str = "ao";
/// Set per-`loadfile` (a file-local option in the `loadfile` command's own options string), never
/// globally — `05-05` owns the exact encoding rules. Named here anyway since it is still an mpv
/// option name, just one this crate never passes to `set_option`.
pub const OPT_HTTP_HEADER_FIELDS: &str = "http-header-fields";

// Runtime properties (`Mpv::set_property`/`get_property`, and `observe_property`'s names).
pub const PROP_PAUSE: &str = "pause";
pub const PROP_VOLUME: &str = "volume";
pub const PROP_MUTE: &str = "mute";
pub const PROP_AUDIO_DEVICE: &str = "audio-device";
/// `09-03`, `docs/05-audio-engine.md` §5 — the equalizer's own `anequalizer` chain is installed
/// and updated by resetting this property (`mpv::filters::apply`/`clear`).
pub const PROP_AF: &str = "af";
pub const PROP_TIME_POS: &str = "time-pos";
pub const PROP_DURATION: &str = "duration";
pub const PROP_CORE_IDLE: &str = "core-idle";
pub const PROP_EOF_REACHED: &str = "eof-reached";
/// The bare `audio-params` property is an mpv `Node` (a map with `format`/`samplerate`/
/// `channel-count` keys) — `libmpv2::events::PropertyData` only extracts `Str`/`OsdStr`/`Flag`/
/// `Int64`/`Double` (`Node` hits its own `unimplemented!()`), so `05-04` observes the three
/// sub-fields below individually instead (mpv supports this "property expansion" natively) and
/// never this parent name. Kept only as a documented name, not in `OBSERVED_PROPERTIES`.
pub const PROP_AUDIO_PARAMS: &str = "audio-params";
pub const PROP_AUDIO_PARAMS_FORMAT: &str = "audio-params/format";
pub const PROP_AUDIO_PARAMS_SAMPLERATE: &str = "audio-params/samplerate";
pub const PROP_AUDIO_PARAMS_CHANNELS: &str = "audio-params/channel-count";
pub const PROP_AUDIO_BITRATE: &str = "audio-bitrate";
pub const PROP_AUDIO_CODEC_NAME: &str = "audio-codec-name";
/// Also an mpv `Node` (an array of maps) — same limitation as `audio-params` above. Not observed;
/// `EnumerateDevices` (a later task) reads it directly on demand instead of through the
/// observe/event mechanism, at whichever point it needs it. See `docs/12-decisions.md`.
pub const PROP_AUDIO_DEVICE_LIST: &str = "audio-device-list";
/// `demuxer-cache-state` (`docs/05-audio-engine.md` §3's own name) is *also* a `Node` (cache
/// ranges, durations, ...) with no plain percentage in it directly — `cache-buffering-state`, a
/// separate plain `Int64` property mpv already computes (0-100), is what `AudioEvent::Buffering`
/// actually needs and is used instead. Verified against a real running mpv (`mpv-tests`
/// environment): `get_property`/`observe_property` both work on it exactly like any other
/// integer property.
pub const PROP_DEMUXER_CACHE_STATE: &str = "demuxer-cache-state";
pub const PROP_CACHE_BUFFERING_STATE: &str = "cache-buffering-state";
pub const PROP_PATH: &str = "path";
pub const PROP_PLAYLIST_POS: &str = "playlist-pos";

/// Every property `05-04`'s event pump actually installs an `observe_property` call for.
/// `docs/05-audio-engine.md` §3's own list names `audio-params` and `audio-device-list` directly;
/// both are `Node`-typed and unobservable through this binding (see their own doc comments above)
/// — replaced with `audio-params`'s three observable sub-fields plus `audio-codec-name` (needed
/// for `Format`'s codec field, per §2, but missing from §3's own list entirely), and
/// `cache-buffering-state` in place of the also-`Node` `demuxer-cache-state`.
pub const OBSERVED_PROPERTIES: &[&str] = &[
    PROP_TIME_POS,
    PROP_DURATION,
    PROP_PAUSE,
    PROP_CORE_IDLE,
    PROP_EOF_REACHED,
    PROP_AUDIO_PARAMS_FORMAT,
    PROP_AUDIO_PARAMS_SAMPLERATE,
    PROP_AUDIO_PARAMS_CHANNELS,
    PROP_AUDIO_BITRATE,
    PROP_AUDIO_CODEC_NAME,
    PROP_CACHE_BUFFERING_STATE,
    PROP_VOLUME,
    PROP_MUTE,
    PROP_PATH,
    PROP_PLAYLIST_POS,
];

// mpv input-command names (`Mpv::command`).
pub const CMD_LOADFILE: &str = "loadfile";
pub const CMD_STOP: &str = "stop";
pub const CMD_SEEK: &str = "seek";

pub const LOADFILE_REPLACE: &str = "replace";
pub const LOADFILE_APPEND: &str = "append";
pub const SEEK_ABSOLUTE: &str = "absolute";
pub const SEEK_RELATIVE: &str = "relative";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_properties_is_non_empty_and_deduplicated() {
        assert!(!OBSERVED_PROPERTIES.is_empty());
        let mut sorted = OBSERVED_PROPERTIES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            OBSERVED_PROPERTIES.len(),
            "a property name is listed twice in OBSERVED_PROPERTIES"
        );
    }
}
