//! Pure domain model and state machine for loxia. No I/O, no async runtime, no rendering —
//! see docs/01-architecture.md §3.1 for the crate boundary rules this enforces.

pub mod action;
pub mod config;
pub mod discography;
pub mod effect;
pub mod error;
pub mod event;
pub mod keymap;
pub mod model;
pub mod paths;
pub mod queue;
pub mod reducer;
pub mod state;
#[cfg(feature = "test-support")]
pub mod test_support;
pub mod theme;

/// A wall-clock instant. Used throughout for `played_at`, `saved_at`, `created_at`, and similar
/// fields — deliberately a single alias so the whole workspace shares one time type instead of
/// mixing `std::time::SystemTime`, `chrono`, and `jiff` at different boundaries.
pub type Timestamp = jiff::Timestamp;

/// Local wall-clock `(hour, minute)` for `ts`, using the system timezone. `loxia-tui` (which may
/// depend on nothing but `loxia-core`, `docs/13-dependencies.md`) needs this to render the header
/// clock (`04-05`) without taking a direct `jiff` dependency of its own — this is the one place
/// `loxia-core` touches `jiff::tz` for it.
pub fn local_hour_minute(ts: Timestamp) -> (u8, u8) {
    let zoned = ts.to_zoned(jiff::tz::TimeZone::system());
    (zoned.hour() as u8, zoned.minute() as u8)
}
