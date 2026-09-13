//! Playback engine: the AudioBackend abstraction, the libmpv2-backed implementation, and a
//! deterministic mock. Depends only on loxia-core — see docs/01-architecture.md §3.3.

pub mod backend;
pub mod device;
pub mod eq;
pub mod error;
pub mod gapless;
pub mod mock;
pub mod mpv;
pub mod replaygain;
