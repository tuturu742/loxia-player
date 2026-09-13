//! Audio filter graph construction: installs and updates the equalizer's `anequalizer` chain
//! against a real mpv handle (`docs/05-audio-engine.md` §5) — the mpv-facing half of `eq.rs`'s
//! pure string-building.

use super::handle::MpvOps;
use super::props::PROP_AF;
use crate::backend::EqCurve;
use crate::eq;
use crate::error::AudioError;

/// Reinstalls the whole `anequalizer` chain via the `af` property, at the gains `curve` gives.
///
/// `docs/05-audio-engine.md` §5 asks to "install the chain once, then mutate gains" via
/// `af-command`, since rebuilding it per keypress was expected to restart playback. Verified
/// against a real mpv instance available in this session's own environment
/// (`docs/12-decisions.md`): `af-command`'s own `change` command against a `lavfi`-wrapped
/// `anequalizer` returned `MPV_ERROR_COMMAND` in every spelling tried, while resetting this
/// property directly, mid-playback, measurably did **not** restart the track or reset
/// `time-pos`. So there is only this one path — install and every subsequent gain/preset/bypass
/// change all call this same function; `eq::band_command`/`af-command` stay unused by the real
/// engine but exist, tested, for a future mpv/FFmpeg build where that route might work.
pub(crate) fn apply(mpv: &dyn MpvOps, curve: &EqCurve) -> Result<(), AudioError> {
    mpv.set_property_str(PROP_AF, &eq::filter_string(curve))
        .map_err(|source| AudioError::Command {
            cmd: PROP_AF,
            detail: source.to_string(),
        })
}

/// Removes the chain entirely — EQ disabled.
pub(crate) fn clear(mpv: &dyn MpvOps) -> Result<(), AudioError> {
    mpv.set_property_str(PROP_AF, "")
        .map_err(|source| AudioError::Command {
            cmd: PROP_AF,
            detail: source.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A minimal `MpvOps` double recording the last `set_property_str` call — just enough for
    /// this module's own tests, distinct from `handle.rs`'s own, much larger `RecordingMpv`
    /// (which carries state — `current_device`, `fail_device`, ... — this module has no use for).
    #[derive(Default)]
    struct RecordingMpv {
        last: Mutex<Option<(String, String)>>,
    }

    impl super::MpvOps for RecordingMpv {
        fn command(&self, _name: &str, _args: &[&str]) -> libmpv2::Result<()> {
            Ok(())
        }
        fn set_property_str(&self, name: &str, value: &str) -> libmpv2::Result<()> {
            *self.last.lock().unwrap() = Some((name.to_string(), value.to_string()));
            Ok(())
        }
        fn set_property_bool(&self, _name: &str, _value: bool) -> libmpv2::Result<()> {
            Ok(())
        }
        fn set_property_f64(&self, _name: &str, _value: f64) -> libmpv2::Result<()> {
            Ok(())
        }
        fn get_property_f64(&self, _name: &str) -> libmpv2::Result<f64> {
            Ok(0.0)
        }
        fn get_property_str(&self, _name: &str) -> libmpv2::Result<String> {
            Ok(String::new())
        }
    }

    #[test]
    fn apply_resets_the_whole_af_property() {
        let mpv = RecordingMpv::default();
        let curve = EqCurve { gains: [0.0; 10] };
        apply(&mpv, &curve).unwrap();
        assert_eq!(
            *mpv.last.lock().unwrap(),
            Some((PROP_AF.to_string(), eq::filter_string(&curve)))
        );
    }

    #[test]
    fn clear_sets_af_empty() {
        let mpv = RecordingMpv::default();
        clear(&mpv).unwrap();
        assert_eq!(
            *mpv.last.lock().unwrap(),
            Some((PROP_AF.to_string(), String::new()))
        );
    }
}
