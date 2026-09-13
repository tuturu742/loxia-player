//! Emby REST and WebSocket client. Depends only on loxia-core for domain types —
//! see docs/01-architecture.md §3.2.

pub mod auth;
pub mod client;
pub mod dto;
pub mod endpoints;
pub mod error;
pub mod query;
pub mod retry;
pub mod stream;
pub mod ws;
