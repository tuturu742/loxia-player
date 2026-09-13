# 05-01 · Backend trait and types

**Phase:** 05 — Audio MVP · **Agent:** C · **Size:** S
**Prerequisites:** `03-03`
**Reference:** `docs/05-audio-engine.md` §§1–2

## Goal
Define the audio abstraction — the trait, commands, and events — before touching mpv. Everything
above this layer, including the whole test suite, talks only to the trait.

## Files
- `crates/loxia-audio/src/backend.rs`, `error.rs`, `lib.rs`

## Specification

```
pub trait AudioBackend: Send {
    fn send(&self, cmd: AudioCommand) -> Result<(), AudioError>;
    fn subscribe(&self) -> mpsc::UnboundedReceiver<AudioEvent>;
}
```

`AudioCommand` and `AudioEvent` exactly as tabulated in `docs/05-audio-engine.md` §2.
`AudioCommand` derives `Debug, Clone, PartialEq`; `AudioEvent` derives `Debug, Clone, PartialEq`.

`AudioCommand::Load` carries `{ url: String, headers: Vec<(String,String)>, start_at: Duration,
gain_db: Option<f32> }`. The URL is a plain `String` here, not a `StreamUrl`, because `loxia-audio`
must not depend on `loxia-emby`. **Its `Debug` impl must redact anything after `api_key=`** — this
type is logged constantly and would otherwise leak the token on every load.

```
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    LibraryNotFound { hint: String },   // hint is the per-OS install instruction
    Init(String),
    Command { cmd: &'static str, source: String },
    Load { url_redacted: String, reason: String },
    DeviceUnavailable { id: String },
    Shutdown,
}
```

`LibraryNotFound.hint` is built per platform: `brew install mpv` on macOS, the distribution package
name on Linux, and a note that the Windows installer bundles `mpv-1.dll`. Producing a useful message
here is what turns a confusing startup crash into a one-line fix for the user.

`loxia-audio` depends on `loxia-core` for `AudioFormat`, `PlayStatus`, `SeekTarget`, and
`ReplayGainMode`. It does **not** define its own copies.

Also define:
```
pub struct AudioDevice { pub id: String, pub description: String, pub driver: String }
pub struct EqCurve { pub gains: [f32; 10] }
pub const EQ_BANDS_HZ: [u32; 10] = [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
```

## Acceptance
- `load_command_debug_redacts_api_key` — a URL containing `api_key=secret` formats without `secret`.
- `library_not_found_hint_is_platform_specific` — three `#[cfg]`-gated assertions.
- `audio_error_display_is_single_sentence`
- `backend_trait_is_object_safe` — a static assertion that `Box<dyn AudioBackend>` compiles.
- `eq_bands_are_iso_standard`
- `command_and_event_are_send_sync`

## Done when
The global DoD in `tasks/README.md` is satisfied.
