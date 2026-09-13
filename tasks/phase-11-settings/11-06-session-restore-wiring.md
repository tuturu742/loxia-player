# 11-06 · Session restore wiring

**Phase:** 11 — Settings · **Agent:** A · **Size:** S
**Prerequisites:** `08-08`, `11-01`
**Reference:** `docs/06-cache-and-offline.md` §8, `docs/12-decisions.md` §6

## Goal
Connect session persistence to bootstrap and shutdown, so a restart returns the user to where they
were.

## Files
- `crates/loxia-player/src/bootstrap.rs` (extend)
- `crates/loxia-player/src/runtime.rs` (extend)

## Specification

**Bootstrap order** — this sequence matters:
1. Load config and resolve paths.
2. Build the offline index and cache manifest.
3. Load `history.json` into `AppState.history`.
4. If `ui.restore_session`, load and validate the snapshot (task `08-08`).
5. Apply it: queue, position, tab, Zen state, volume, quality profile, EQ.
6. Connect to the server.
7. Seed the active tab's columns **only if** the restored tab has no columns.

Restoring **before** connecting means the UI is usable immediately, and a slow or failed connection
still leaves the user with their queue rather than an empty screen.

**Playback restores paused** at `position_secs`, with `⏸ resumed at 01:24` in the player bar.
`ui.restore_autoplay = true` instead emits `Effect::Audio(Load { start_at })` followed by `Play`.

**Shutdown order:**
1. Emit the final `Stopped` playback report (task `06-07`).
2. Emit `Effect::Cache(PersistSession)` with the current snapshot.
3. Await worker drain, bounded at 2 seconds.
4. Restore the terminal and flush the log guard.

A hung worker must not prevent exit; the bound is what guarantees that.

**Settings controls** in the Interface section: `restore_session` and `restore_autoplay` toggles,
plus a `Clear saved session` action that deletes `session.json` and empties the queue after a
`Confirm`.

**Toast on restore:** `restored 14 tracks — paused at 01:24`, `Info`. A user who does not know the
feature exists otherwise finds a queue they did not create.

## Acceptance
- `restore_happens_before_connect`
- `restore_applies_all_snapshot_fields`
- `restore_leaves_playback_paused`
- `restore_autoplay_starts_playback`
- `restore_toast_names_count_and_position`
- `seed_skipped_when_restored_tab_has_columns`
- `shutdown_emits_final_report_then_snapshot`
- `shutdown_bounded_at_two_seconds`
- `clear_saved_session_requires_confirm_and_deletes_file`
- `restore_disabled_by_config`
- `failed_connection_still_leaves_queue_restored`
- Manual, pasted into the PR: play a track, quit at 01:24, relaunch, and confirm the queue,
  position, tab, and volume return with playback paused.

## Done when
The global DoD in `tasks/README.md` is satisfied.
