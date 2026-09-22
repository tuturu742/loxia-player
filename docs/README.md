# Documentation

`docs/` describes the system as it is; planned work lives in [`ROADMAP.md`](ROADMAP.md).

## Design & architecture

| File | Question it answers | Who should read it |
|---|---|---|
| [`01-architecture.md`](01-architecture.md) | How are the workspace crates and runtime boundaries organised? | New contributors |
| [`02-data-model.md`](02-data-model.md) | Which domain, configuration, and persisted data types does loxia use? | Contributors changing core state or configuration |
| [`03-emby-api.md`](03-emby-api.md) | How does loxia communicate with Emby? | Contributors changing server integration |
| [`04-state-and-input.md`](04-state-and-input.md) | How do actions, state, reducers, effects, and keybindings interact? | Contributors changing behaviour or input |
| [`05-audio-engine.md`](05-audio-engine.md) | How does playback and audio processing work? | Contributors changing audio playback |
| [`06-cache-and-offline.md`](06-cache-and-offline.md) | How does loxia store downloads and offline data? | Contributors changing persistence or offline behaviour |
| [`07-ui-spec.md`](07-ui-spec.md) | How is the terminal interface composed and rendered? | Contributors changing the TUI |

## Reference

| File | Question it answers | Who should read it |
|---|---|---|
| [`10-testing-and-ci.md`](10-testing-and-ci.md) | Which automated checks protect the workspace and how are they run? | Contributors and maintainers |
| [`11-packaging.md`](11-packaging.md) | How is loxia distributed and what runtime components does it require? | Release maintainers and packagers |
| [`12-decisions.md`](12-decisions.md) | Which implementation decisions differ from earlier designs, and why? | Contributors changing documented behaviour |
| [`13-dependencies.md`](13-dependencies.md) | Which dependency policies and versions govern the workspace? | Contributors changing `Cargo.toml` files |

## Planning

| File | Question it answers | Who should read it |
|---|---|---|
| [`ROADMAP.md`](ROADMAP.md) | What work remains outside the implemented reference system? | Contributors and maintainers |
