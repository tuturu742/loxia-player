# 05-06 · Audio worker

**Phase:** 05 — Audio MVP · **Agent:** C · **Size:** S
**Prerequisites:** `05-05`, `03-08`
**Reference:** `docs/01-architecture.md` §4

## Goal
Wire the engine into the runtime: translate `Effect::Audio` into `AudioCommand`, and `AudioEvent`
into `Event`. After this, pressing a key produces sound.

## Files
- `crates/loxia-player/src/workers/audio.rs`
- `crates/loxia-player/src/bootstrap.rs` (extend)

## Specification

```
pub fn spawn(backend: Box<dyn AudioBackend>, effects: UnboundedReceiver<Effect>,
             events: UnboundedSender<Event>) -> JoinHandle<()>;
```

Two loops in one task via `select!`: drain effects into `backend.send`, and forward the backend's
events into the shared Event channel after mapping.

**Effect → command** is one-to-one for every `Effect::Audio` variant. A `send` failure becomes
`Event::Audio(EngineError(..))` rather than being logged and dropped; the user needs to know the
engine stopped responding.

**Event → Action** mapping:
| `AudioEvent` | `Action` |
| :-- | :-- |
| `StatusChanged` | `Action::Audio(StatusChanged)` |
| `Position` | `Action::Audio(PositionChanged)` |
| `Format` | `Action::Audio(FormatDetected)` |
| `TrackEnded` | `Action::Audio(TrackEnded)` |
| `Devices` | `Action::Data(DevicesLoaded)` |
| `Buffering` | `Action::Audio(StatusChanged(Buffering))` |
| `Error` | `Action::Audio(EngineError)` |

**Backend selection** in bootstrap: `MockEngine` when `--no-audio`, otherwise `MpvEngine`. A
`LibraryNotFound` at construction propagates to `main` for the message in task `05-05`.

**Reducer additions** (`reducer/player.rs`, minimal for this phase): handle the `Action::Audio`
variants by updating `PlayerState`. Handle `Player::PlayPause`, `Seek`, `SetVolume`, `ToggleMute` by
emitting the corresponding effect and **changing nothing else** — the mirror updates only when the
engine confirms. This is the rule from `docs/04-state-and-input.md` §4.6 and the reason the UI never
lies about what is playing.

Add a hidden `--play-url <URL>` flag that loads a URL directly at startup, so the engine can be
exercised before the queue exists.

## Acceptance
- `effect_to_command_mapping` — table test over every `Effect::Audio` variant, using `MockEngine`
  and asserting `commands()`.
- `event_to_action_mapping` — table test over every `AudioEvent`.
- `play_pause_does_not_mutate_player_state_directly` — dispatch `PlayPause`, assert `player.status`
  is unchanged and exactly one effect was emitted.
- `status_updates_only_on_audio_event`
- `send_failure_becomes_engine_error_action`
- `no_audio_flag_selects_mock`
- Manual, pasted into the PR: `cargo run -p loxia-player -- --play-url "<direct stream url>"` plays audio;
  the player bar shows a moving position and the correct format line; `Space` pauses and resumes.

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 05's exit criteria in
`docs/08-roadmap.md` are met.
