# 09-06 · Quality profiles

**Phase:** 09 — Advanced audio · **Agent:** C · **Size:** S
**Prerequisites:** `09-02`, `08-03`
**Reference:** `docs/03-emby-api.md` §5, `docs/06-cache-and-offline.md` §3

## Goal
`q` cycles between direct FLAC and three transcode tiers, reloading the current track at the same
position.

## Files
- `crates/loxia-core/src/reducer/player.rs` (extend)

## Specification

Cycle order: `Direct → TranscodeHigh (320k) → TranscodeMed (192k) → TranscodeLow (96k) → Direct`.

**On change:**
1. Update `player.quality_profile` and persist to `transcode.mode`, debounced.
2. If a track is playing, capture `player.position`, emit `Effect::Cache(EnsureCached)` for the new
   `(item, profile)` key, then `Effect::Audio(Load { start_at: position })` with the new URL.
3. Toast the new profile with its bitrate: `quality: 192 kbps Opus`.

The reload is audible as a brief pause. That is expected and unavoidable — the server has to start a
new transcode — and the toast makes the cause obvious rather than looking like a glitch.

**Cache keys include the profile** (task `08-01`), so switching from Direct to Med192 fetches a
different file rather than serving the cached FLAC under the wrong label. Switching back to a
profile already cached is instant.

**Preload invalidation.** The preloaded next entry was queued at the old profile, so changing
quality must re-emit `Effect::Audio(Preload)` for the new one; otherwise the next track plays at the
previous quality with no indication.

**Bit-perfect** forces `Direct` and refuses cycling with `quality is fixed in bit-perfect mode`
(task `09-02`).

**Offline.** Cycling is refused with `quality changes need a connection` when the current track is
served from cache or downloads — the local file is whatever it is, and pretending otherwise would
mislabel the player bar.

`transcode.target_codec` (`mp3`, `aac`, or `opus`) selects the codec for all three transcode tiers
and is a settings-only option, not part of the cycle; four tiers times three codecs is too many
states for one key.

## Acceptance
- `cycle_order_is_direct_high_med_low`
- `cycle_reloads_at_same_position`
- `quality_cycle_persists_to_config`
- `cache_key_changes_with_profile`
- `switching_back_to_cached_profile_makes_no_request`
- `preload_reissued_after_quality_change`
- `cycle_refused_in_bit_perfect_mode`
- `cycle_refused_offline_for_local_file`
- `toast_names_bitrate_and_codec`
- `no_track_playing_changes_profile_without_load`

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 09's exit criteria in
`docs/08-roadmap.md` are met.
