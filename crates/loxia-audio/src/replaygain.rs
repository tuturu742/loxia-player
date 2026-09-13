//! ReplayGain mode mapping and normalization fallback (`docs/05-audio-engine.md` §6).

use loxia_core::config::ReplayGainMode;

/// mpv's own `replaygain` property values — `Album` → `"album"`, `Track` → `"track"`,
/// `Off` → `"no"`.
pub fn mode_property(mode: ReplayGainMode) -> &'static str {
    match mode {
        ReplayGainMode::Album => "album",
        ReplayGainMode::Track => "track",
        ReplayGainMode::Off => "no",
    }
}

/// Decides which gain path applies to the current track and mode.
///
/// Defined in `loxia_core::state::player` (not here) and re-exported: `reducer::player` needs to
/// call this too, to decide what to record in `PlayerState.applied_gain`, and `loxia-core` cannot
/// depend on `loxia-audio`. Unlike `09-03`'s `clamp_eq_gain` — genuinely needed independently on
/// both sides of that boundary — this function only ever touches `loxia-core`'s own types
/// (`ReplayGainMode`, `ReplayGainInfo`, `AppliedGain`), so there's no reason to keep two copies in
/// sync: one definition, re-exported here to satisfy this task's own file/signature requirement.
pub use loxia_core::state::player::resolve_gain;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_maps_to_mpv_property() {
        assert_eq!(mode_property(ReplayGainMode::Album), "album");
        assert_eq!(mode_property(ReplayGainMode::Track), "track");
        assert_eq!(mode_property(ReplayGainMode::Off), "no");
    }

    // `resolve_gain`'s own acceptance tests (`resolve_gain_prefers_tags_over_normalization`,
    // `resolve_gain_falls_back_to_normalization`, `resolve_gain_none_when_neither_available`,
    // `off_mode_applies_no_gain_even_with_tags`) live alongside its real definition in
    // `loxia_core::state::player`, not duplicated here — see that module's own test suite.
}
