# Source traceability

This map routes a reader from a system concern to its primary implementation.
The public module documentation and tests remain the detailed source of truth.

| Concern | Primary location |
|---|---|
| Workspace composition | `Cargo.toml`, crate `Cargo.toml` files, and [`01-architecture.md`](01-architecture.md) |
| Domain models and configuration | `crates/loxia-core/src/model`, `config`, and `paths` |
| Actions, effects, events, and reducers | `crates/loxia-core/src/action.rs`, `effect.rs`, `event.rs`, and `reducer` |
| Key bindings | `crates/loxia-core/src/keymap` |
| Queue behaviour | `crates/loxia-core/src/queue` and `state/queue.rs` |
| Emby protocol adapter | `crates/loxia-emby/src` |
| Audio playback | `crates/loxia-audio/src` |
| Cache and offline persistence | `crates/loxia-cache/src` |
| Terminal rendering | `crates/loxia-tui/src` |
| Runtime composition and workers | `crates/loxia-player/src` |
| Themes and factory EQ presets | `assets/themes` and `assets/eq_presets.toml` |
| Automated checks | `.github/workflows/ci.yml`, `justfile`, and [`10-testing-and-ci.md`](10-testing-and-ci.md) |
| Recorded design choices | [`12-decisions.md`](12-decisions.md) |
| Dependency policy | [`13-dependencies.md`](13-dependencies.md) |

Use [`ROADMAP.md`](ROADMAP.md) for work that is not represented by a current
implementation location.
