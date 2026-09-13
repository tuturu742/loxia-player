//! 100-nanosecond tick <-> Duration conversion. This is the single conversion point — nothing
//! else in the workspace may multiply or divide by 10,000,000.

use std::time::Duration;

pub const TICKS_PER_SECOND: i64 = 10_000_000;

/// Saturating; negative ticks clamp to `Duration::ZERO`.
pub fn ticks_to_duration(ticks: i64) -> Duration {
    if ticks <= 0 {
        return Duration::ZERO;
    }
    let secs = (ticks / TICKS_PER_SECOND) as u64;
    let remainder = ticks % TICKS_PER_SECOND;
    let nanos = (remainder * 100) as u32;
    Duration::new(secs, nanos)
}

/// Saturating at `i64::MAX`.
pub fn duration_to_ticks(d: Duration) -> i64 {
    let secs_ticks = i128::from(d.as_secs()) * i128::from(TICKS_PER_SECOND);
    let nanos_ticks = i128::from(d.subsec_nanos()) / 100;
    (secs_ticks + nanos_ticks).min(i128::from(i64::MAX)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_ticks_clamp_to_zero() {
        assert_eq!(ticks_to_duration(-1), Duration::ZERO);
        assert_eq!(ticks_to_duration(i64::MIN), Duration::ZERO);
    }

    #[test]
    fn zero_ticks_is_zero_duration() {
        assert_eq!(ticks_to_duration(0), Duration::ZERO);
    }

    #[test]
    fn known_value_roundtrips_exactly() {
        // 4089730610 ticks, taken from the `album_tracks.json` fixture's RunTimeTicks.
        let ticks = 4_089_730_610_i64;
        let d = ticks_to_duration(ticks);
        assert_eq!(d, Duration::new(408, 973_061_000));
        assert_eq!(duration_to_ticks(d), ticks);
    }

    #[test]
    fn saturates_at_i64_max() {
        let huge = Duration::new(u64::MAX, 0);
        assert_eq!(duration_to_ticks(huge), i64::MAX);
    }

    proptest::proptest! {
        #[test]
        fn ticks_roundtrip(ticks in 0i64..=i64::MAX / 2) {
            let back = duration_to_ticks(ticks_to_duration(ticks));
            proptest::prop_assert!((back - ticks).abs() <= 1);
        }
    }
}
