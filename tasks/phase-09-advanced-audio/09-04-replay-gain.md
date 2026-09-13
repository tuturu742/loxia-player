# 09-04 · ReplayGain

**Phase:** 09 — Advanced audio · **Agent:** C · **Size:** S
**Prerequisites:** `09-03`
**Reference:** `docs/05-audio-engine.md` §6

## Goal
Volume normalisation in Album, Track, and Off modes, with a fallback for files that carry no
ReplayGain tags.

## Files
- `crates/loxia-audio/src/replaygain.rs`

## Specification

**Primary path.** Map `ReplayGainMode` to mpv's `replaygain` property: `Album` → `album`,
`Track` → `track`, `Off` → `no`. Also set `replaygain-preamp` from
`audio.replaygain_preamp_db` and `replaygain-clip = yes`, which prevents positive gain from
clipping.

**Fallback.** When a track carries no ReplayGain tags, use Emby's `normalizationGain` from the media
source (captured in task `02-09`), applied as `AudioCommand::Load { gain_db }` — which sets mpv's
`volume-gain` for that file. When neither is available, no gain is applied.

```
pub fn resolve_gain(mode: ReplayGainMode, track_rg: Option<&ReplayGainInfo>,
                    normalization_db: Option<f32>) -> AppliedGain;
pub enum AppliedGain { Tags(ReplayGainMode), Normalization(f32), None }
```

**Transparency.** `PlayerState.applied_gain_db` records the result, and the inspector displays which
path was taken — `ReplayGain (album): −6.2 dB`, `Emby normalization: −4.1 dB`, or `no gain`.
Normalisation that silently does nothing on half a library is the kind of thing users spend an
evening debugging; showing the source removes the mystery.

**Album versus Track.** Album mode preserves the relative loudness *within* a record, which is the
point of listening to an album; Track mode levels every track independently, which suits shuffled
listening. Both are correct for different uses, so neither is "better" — the setting help text says
so rather than recommending one.

`CycleReplayGain` (`r`) cycles `Album → Track → Off` and toasts the new mode with the gain that will
apply to the current track. It is refused while bit-perfect is on (task `09-02`).

Changing the mode takes effect on the **current** track immediately, not only on the next one.

## Acceptance
- `mode_maps_to_mpv_property` — table test over the three modes.
- `preamp_and_clip_are_set`
- `resolve_gain_prefers_tags_over_normalization`
- `resolve_gain_falls_back_to_normalization`
- `resolve_gain_none_when_neither_available`
- `off_mode_applies_no_gain_even_with_tags`
- `applied_gain_recorded_in_player_state`
- `inspector_shows_gain_source` — snapshot per `AppliedGain` variant.
- `cycle_order_is_album_track_off`
- `cycle_refused_in_bit_perfect_mode`
- `mode_change_applies_to_current_track`

## Done when
The global DoD in `tasks/README.md` is satisfied.
