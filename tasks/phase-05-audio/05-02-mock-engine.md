# 05-02 · Mock engine

**Phase:** 05 — Audio MVP · **Agent:** C · **Size:** M
**Prerequisites:** `05-01`
**Reference:** `docs/05-audio-engine.md` §§1, 9

## Goal
A deterministic fake backend driven by a virtual clock. It is what lets the entire test suite, and
all UI work, run with no mpv and no sound card.

## Files
- `crates/loxia-audio/src/mock.rs`

## Specification

```
pub struct MockEngine { .. }
impl MockEngine {
    pub fn new() -> (MockEngine, MockControl);
}
pub struct MockControl {
    pub fn advance(&self, by: Duration);          // drive the virtual clock; emits Position events
    pub fn finish_track(&self);                   // emit TrackEnded { natural: true }
    pub fn fail_next_load(&self, reason: &str);
    pub fn stall(&self, yes: bool);               // accept commands, emit nothing
    pub fn remove_device(&self, id: &str);
    pub fn set_devices(&self, d: Vec<AudioDevice>);
    pub fn commands(&self) -> Vec<AudioCommand>;  // everything received, in order
}
```

**No real time anywhere.** Position advances only through `advance()`. A mock that spawns a timer
reintroduces exactly the flakiness this type exists to remove.

Simulated behaviour:
- `Load` → `StatusChanged(Loading)`, then `Format(..)` from a per-track table the test can seed,
  then `StatusChanged(Playing)` — unless `fail_next_load` was armed, which emits
  `Error(AudioError::Load { .. })` and leaves the status `Stopped`.
- `advance()` emits `Position` events **throttled to 4 Hz of virtual time**, matching the real
  engine, so tests exercise the same event cadence.
- Reaching the loaded duration emits `TrackEnded { natural: true }` automatically; `finish_track()`
  forces it early.
- `Stop` → `TrackEnded { natural: false }` then `StatusChanged(Stopped)`.
- `EnumerateDevices` → `Devices(..)` from the seeded list.
- `SetDevice` for an id not in the list → `Error(DeviceUnavailable)`.
- `stall(true)` records commands but emits nothing, for the "engine is wedged" tests.

`commands()` is the assertion surface: tests check what the reducer asked for, not just what the
state became.

## Acceptance
- `load_emits_loading_format_playing_in_order`
- `advance_emits_throttled_position_events` — advancing 1 s of virtual time yields 4 events.
- `reaching_duration_emits_track_ended_natural`
- `stop_emits_track_ended_not_natural`
- `fail_next_load_emits_error_and_stays_stopped`
- `stall_records_commands_but_emits_nothing`
- `set_device_unknown_id_errors`
- `mock_is_deterministic` — the same command and advance sequence produces byte-identical event
  sequences across 100 runs.
- `mock_uses_no_wall_clock` — a test that sleeps for 100 ms of real time and asserts no `Position`
  event was emitted.

## Done when
The global DoD in `tasks/README.md` is satisfied.
