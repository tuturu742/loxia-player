# Documentation

`docs/` describes the system as it is; planned work lives in [`ROADMAP.md`](ROADMAP.md).

## Design & architecture

| File | Question it answers | Who should read it |
| :-- | :-- | :-- |
| [`README.md`](README.md) | Where do I start in the reference documentation? | Everyone |
| [`01-architecture.md`](01-architecture.md) | How are the workspace crates and runtime responsibilities arranged? | New contributors and maintainers |
| [`02-data-model.md`](02-data-model.md) | Which domain, configuration, and persistent data types does loxia use? | Contributors working on core state or configuration |
| [`04-state-and-input.md`](04-state-and-input.md) | How do actions, effects, events, reducers, and key input interact? | Contributors working on state, input, or runtime dispatch |
| [`05-audio-engine.md`](05-audio-engine.md) | How does playback and the libmpv-backed audio engine work? | Contributors working on audio or playback |
| [`06-cache-and-offline.md`](06-cache-and-offline.md) | How do cache, downloads, offline data, and persisted sessions work? | Contributors working on storage or offline behaviour |
| [`07-ui-spec.md`](07-ui-spec.md) | How is the terminal interface structured and rendered? | Contributors working on the TUI |

## Reference

| File | Question it answers | Who should read it |
| :-- | :-- | :-- |
| [`03-emby-api.md`](03-emby-api.md) | How does loxia communicate with an Emby server? | Contributors working on `loxia-emby` or network workers |
| [`09-traceability.md`](09-traceability.md) | Where do implemented system responsibilities live in the workspace? | Maintainers and reviewers |
| [`10-testing-and-ci.md`](10-testing-and-ci.md) | Which checks, fixtures, snapshots, and CI jobs protect the system? | Contributors and CI maintainers |
| [`11-packaging.md`](11-packaging.md) | What does the project currently package, license, and distribute? | Release and packaging maintainers |
| [`12-decisions.md`](12-decisions.md) | Why does the implementation make its significant technical choices? | Contributors changing documented behaviour |
| [`13-dependencies.md`](13-dependencies.md) | Which dependencies and dependency rules does the workspace use? | Contributors changing `Cargo.toml` files |
| [`14-manual-test-plan.md`](14-manual-test-plan.md) | Which user-facing behaviours need manual verification? | Testers and release maintainers |

## Planning

| File | Question it answers | Who should read it |
| :-- | :-- | :-- |
| [`ROADMAP.md`](ROADMAP.md) | What work remains outside the implemented reference set? | Maintainers and contributors selecting future work |
