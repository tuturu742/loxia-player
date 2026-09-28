//! Dual-tier cache (rolling LRU + permanent downloads), offline browse index, scrobble buffer,
//! and session/history persistence. Depends only on loxia-core.

pub mod downloads;
pub mod error;
pub mod layout;
pub mod lru;
pub mod manifest;
pub mod offline_index;
pub mod scrobble;
pub mod session;
