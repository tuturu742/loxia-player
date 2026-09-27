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
/// clock without taking a direct `jiff` dependency of its own — this is the one place
/// `loxia-core` touches `jiff::tz` for it.
///
/// **Pinned to UTC under `test-support`.** The header clock and the listening-history rows are
/// rendered into `insta` snapshots, and the system zone makes those a function of the machine
/// that generated them: snapshots written at UTC+2 then fail for every contributor in another
/// zone, and in CI, which runs UTC. `docs/10-testing-and-ci.md` §5 asks for a fixed clock on any
/// snapshot path; a fixed *instant* is not enough on its own, the zone has to be fixed too.
/// `test-support` is a dev-only feature no release build ever enables, so a real clock still
/// reads local time.
pub fn local_hour_minute(ts: Timestamp) -> (u8, u8) {
    #[cfg(feature = "test-support")]
    let tz = jiff::tz::TimeZone::UTC;
    #[cfg(not(feature = "test-support"))]
    let tz = jiff::tz::TimeZone::system();

    let zoned = ts.to_zoned(tz);
    (zoned.hour() as u8, zoned.minute() as u8)
}

#[cfg(all(test, feature = "test-support"))]
mod clock_tests {
    use super::*;

    /// The regression this pinning exists for: five widget snapshots used to encode the
    /// generating machine's UTC offset and failed everywhere else.
    #[test]
    fn snapshot_clock_does_not_follow_the_machine_timezone() {
        // 1970-01-01T00:00:00Z — whatever the host zone is, this must read 00:00.
        let (hour, minute) = local_hour_minute(Timestamp::from_second(0).unwrap());
        assert_eq!(
            (hour, minute),
            (0, 0),
            "snapshot rendering must not depend on the host timezone"
        );
    }
}
