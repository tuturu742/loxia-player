//! 10-band ISO equalizer curve to filter string/commands (`docs/05-audio-engine.md` §5).

use loxia_core::config::EqPreset;

use crate::backend::{EQ_BANDS_HZ, EqCurve};

/// `anequalizer`'s own "peaking" (parametric) filter type — the task's own literal example
/// (`t=1`).
const FILTER_TYPE_PEAKING: u8 = 1;

/// `±12 dB`, snapped to the nearest `0.5 dB` (`docs/05-audio-engine.md` §5).
pub fn clamp_gain(db: f32) -> f32 {
    let snapped = (db / 0.5).round() * 0.5;
    snapped.clamp(-12.0, 12.0)
}

/// One octave around `center_hz`: an octave band spans `center_hz / sqrt(2)` to `center_hz *
/// sqrt(2)`, so its width is `center_hz * (sqrt(2) - 1/sqrt(2))` — algebraically the same as
/// `center_hz / sqrt(2)`, since `sqrt(2) - 1/sqrt(2) = 1/sqrt(2)`.
fn octave_width_hz(center_hz: u32) -> f32 {
    center_hz as f32 * std::f32::consts::FRAC_1_SQRT_2
}

fn band_entry(center_hz: u32, gain_db: f32) -> String {
    format!(
        "c0 f={center_hz} w={:.2} g={:.1} t={FILTER_TYPE_PEAKING}",
        octave_width_hz(center_hz),
        gain_db
    )
}

/// The full 10-band `anequalizer` filter graph, `|`-joined (`docs/05-audio-engine.md` §5:
/// "install the chain once, then mutate gains"). mpv has no *native* `anequalizer` filter of its
/// own — it's an FFmpeg/`libavfilter` one — so it must be routed through mpv's `lavfi` bridge:
/// verified against a real mpv instance available in this session's own environment, a bare
/// `anequalizer=...` string set directly to the `af` property fails
/// (`MPV_ERROR_PROPERTY_FORMAT`), while wrapping it `lavfi=[...]` succeeds
/// (`docs/12-decisions.md`).
pub fn filter_string(curve: &EqCurve) -> String {
    let entries: Vec<String> = EQ_BANDS_HZ
        .iter()
        .zip(curve.gains.iter())
        .map(|(&f, &g)| band_entry(f, clamp_gain(g)))
        .collect();
    format!("lavfi=[anequalizer={}]", entries.join("|"))
}

/// `(name, args)` for `af-command`'s own `<command> <argument>` pair, targeting `anequalizer`'s
/// documented `change` runtime command (`<index>|f=<freq>|w=<width>|g=<gain>`) — this task's own
/// given signature.
///
/// **Not used by the real mpv command path**: `mpv::handle::apply_command`'s own `SetEq`
/// handling reinstalls the whole chain via [`filter_string`] instead of calling this. Verified
/// against a real, running mpv/FFmpeg build available in this session's own environment
/// (`docs/12-decisions.md`): every `af-command <label> change <args>` attempt against a live
/// `lavfi`-wrapped `anequalizer` returned `MPV_ERROR_COMMAND` regardless of label/target
/// spelling, while resetting the *whole* `af` property mid-playback measurably did **not** reset
/// or restart the track (`time-pos` continued monotonically through it) — the opposite of what
/// this task's own text predicts ("rebuilding the af chain per keypress makes mpv restart
/// playback"), at least on this build. Kept as a pure, tested, spec-shaped function for a future
/// mpv/FFmpeg build where `af-command` routing into `lavfi` sub-graphs might actually work.
pub fn band_command(band: usize, gain_db: f32) -> (String, String) {
    let gain_db = clamp_gain(gain_db);
    let freq = EQ_BANDS_HZ[band];
    let args = format!(
        "{band}|f={freq}|w={:.2}|g={gain_db:.1}",
        octave_width_hz(freq)
    );
    ("change".to_string(), args)
}

/// The eight factory presets (`docs/05-audio-engine.md` §5), embedded at compile time so no I/O
/// or install-time asset lookup is needed. Parsed once per call rather than cached in a
/// `LazyLock`: this runs at most once per EQ-menu open or config load, and keeping it a plain
/// function keeps the parse failure a normal `panic!` at a predictable call site rather than a
/// lazily-triggered one on first access from an arbitrary thread.
const FACTORY_PRESETS_TOML: &str = include_str!("../../../assets/eq_presets.toml");

#[derive(serde::Deserialize)]
struct PresetsFile {
    presets: Vec<EqPreset>,
}

/// Parses `assets/eq_presets.toml` into the eight factory presets. Panics on a parse failure —
/// that would mean the embedded asset itself is corrupt, which no caller can meaningfully recover
/// from at runtime, not a condition arising from anything a caller passed in.
pub fn factory_presets() -> Vec<EqPreset> {
    let file: PresetsFile =
        toml::from_str(FACTORY_PRESETS_TOML).expect("assets/eq_presets.toml must parse");
    file.presets
}

/// All presets available to the equalizer modal: the eight factory presets followed by the
/// user's own custom presets (`custom`, from `Config.eq_presets`) — the reducer calls this when
/// building the picker list, factory first so a custom preset can never shadow one by name
/// collision, and always appended after them per `docs/05-audio-engine.md` §5.
pub fn all_presets(custom: &[EqPreset]) -> Vec<EqPreset> {
    let mut presets = factory_presets();
    presets.extend(custom.iter().cloned());
    presets
}

/// Looks up a preset by name among `all_presets(custom)` — the reducer calls this to apply the
/// preset a user selected in the equalizer modal.
pub fn find_preset(name: &str, custom: &[EqPreset]) -> Option<EqPreset> {
    all_presets(custom).into_iter().find(|p| p.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::FACTORY_EQ_PRESET_NAMES;

    fn curve(gains: [f32; 10]) -> EqCurve {
        EqCurve { gains }
    }

    #[test]
    fn gain_is_clamped_to_plus_minus_twelve() {
        assert_eq!(clamp_gain(20.0), 12.0);
        assert_eq!(clamp_gain(-20.0), -12.0);
    }

    #[test]
    fn gain_snaps_to_half_db() {
        assert_eq!(clamp_gain(1.2), 1.0);
        assert_eq!(clamp_gain(1.3), 1.5);
    }

    #[test]
    fn filter_string_snapshot() {
        insta::assert_snapshot!("filter_string_flat", filter_string(&curve([0.0; 10])));
        insta::assert_snapshot!(
            "filter_string_boost",
            filter_string(&curve([6.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]))
        );
        insta::assert_snapshot!(
            "filter_string_cut",
            filter_string(&curve([-6.0, -4.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]))
        );
    }

    #[test]
    fn band_command_snapshot() {
        insta::assert_debug_snapshot!("band_command_boost", band_command(0, 6.0));
        insta::assert_debug_snapshot!("band_command_cut", band_command(9, -3.0));
    }

    #[test]
    fn all_factory_presets_parse() {
        let presets = factory_presets();
        assert_eq!(presets.len(), FACTORY_EQ_PRESET_NAMES.len());
        for name in FACTORY_EQ_PRESET_NAMES {
            assert!(
                presets.iter().any(|p| p.name == *name),
                "missing factory preset {name}"
            );
        }
    }

    #[test]
    fn factory_presets_have_ten_gains_in_range() {
        for preset in factory_presets() {
            assert_eq!(preset.gains.len(), 10);
            for gain in preset.gains {
                assert!(
                    (-12.0..=12.0).contains(&gain),
                    "preset {} has an out-of-range gain: {gain}",
                    preset.name
                );
            }
        }
    }

    #[test]
    fn custom_presets_appended_after_factory() {
        let custom = vec![EqPreset {
            name: "my custom preset".to_string(),
            gains: [1.0; 10],
        }];
        let all = all_presets(&custom);
        let factory_len = factory_presets().len();
        assert_eq!(all.len(), factory_len + 1);
        assert_eq!(all[factory_len].name, "my custom preset");
    }

    #[test]
    fn find_preset_looks_up_by_name() {
        let found = find_preset("flat", &[]).expect("flat is a factory preset");
        assert_eq!(found.name, "flat");
        assert!(find_preset("does-not-exist", &[]).is_none());
    }
}
