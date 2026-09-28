//! Playback engine: the AudioBackend abstraction, the libmpv2-backed implementation, and a
//! deterministic mock. Depends only on loxia-core.

pub mod backend;
pub mod device;
pub mod eq;
pub mod error;
pub mod gapless;
pub mod mock;
pub mod mpv;
pub mod replaygain;
