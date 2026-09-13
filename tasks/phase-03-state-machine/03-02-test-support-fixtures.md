# 03-02 · Test-support fixtures

**Phase:** 03 — State machine · **Agent:** A · **Size:** S
**Prerequisites:** `03-01`
**Reference:** `docs/10-testing-and-ci.md` §2

## Goal
Build the shared fixture library and the `Scenario` harness. Every subsequent test in every crate
uses these, so hand-rolled ad-hoc state in later tasks is a review failure.

## Files
- `crates/loxia-core/src/test_support/mod.rs`, `fixtures.rs`, `scenario.rs`

Gated behind the `test-support` feature (declared in `00-02`), so it ships in dev builds only.

## Specification

**Builders** — deterministic, no randomness, no clock:
```
pub fn artist(name: &str) -> Artist;
pub fn album(name: &str, year: u16, artist: &Artist) -> Album;
pub fn appears_on_album(name: &str, year: u16, context: &Artist) -> Album;
pub fn track(name: &str, n: u32, album: &Album, artists: &[&Artist]) -> Track;
pub fn track_with_lyrics(..) -> Track;
```

**`AppState` fixtures:**
| Fixture | Contents |
| :-- | :-- |
| `fixture_empty()` | `AppState::default()` with a theme loaded |
| `fixture_miller_3col()` | Artists → Albums → Tracks, cursor mid-list in column 2 |
| `fixture_miller_5col()` | five deep, for sliding-window tests |
| `fixture_appears_on()` | one artist, 2 primary albums, 2 compilations; the compilations have 10 tracks each, of which 2 feature the artist |
| `fixture_visual_select()` | 3 of 5 tracks selected, visual mode on |
| `fixture_playing_queue()` | 10-entry queue, position 3, `PlayStatus::Playing` |
| `fixture_offline()` | `Connectivity::Offline`, mixed availability |
| `fixture_with_history()` | 50 history entries at fixed timestamps |

All timestamps come from a fixed epoch constant, never `now()`. A fixture that reads the clock makes
every snapshot test flaky.

**Harness:**
```
pub struct Scenario { state: AppState, effects: Vec<Effect> }
impl Scenario {
    pub fn new(state: AppState) -> Self;
    pub fn dispatch(self, a: Action) -> Self;
    pub fn dispatch_all(self, a: impl IntoIterator<Item = Action>) -> Self;
    pub fn state(&self) -> &AppState;
    pub fn effects(&self) -> &[Effect];
    pub fn last_effect(&self) -> Option<&Effect>;
    pub fn assert_toast_contains(&self, needle: &str);
    pub fn assert_no_effects(&self);
}
```
`dispatch` accumulates effects across calls so a multi-step scenario can assert on the whole
sequence.

`Scenario` compiles against a reducer that does not exist yet — task `03-03` defines `Action` and
`Effect`, and `03-06` provides `apply`. Land this task's builders and fixtures first, then wire
`Scenario` once `03-06` is in. Ordering note: implement `fixtures.rs` fully here; `scenario.rs` may
be stubbed with `todo!()` until `03-06` and completed in that PR.

## Acceptance
- `every_fixture_constructs` — one test constructing each fixture and asserting a coherent invariant
  (non-empty columns, cursor in range, queue position valid).
- `fixtures_are_deterministic` — building the same fixture twice yields equal values.
- `fixtures_contain_no_current_timestamps` — every timestamp equals the fixed epoch or is derived
  from it.
- `appears_on_fixture_shape` — 2 primary, 2 appears-on, and exactly 2 artist tracks per compilation.

## Done when
The global DoD in `tasks/README.md` is satisfied.
