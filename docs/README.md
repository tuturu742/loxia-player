# Documentation index

`docs/` describes the system as it is; planned work lives in [`ROADMAP.md`](ROADMAP.md).

## Design & architecture

| Filename | Question answered | Who should read it |
| --- | --- | --- |
| `README.md` | Where do I start in this reference set? | Everyone |
| [`01-architecture.md`](01-architecture.md) | How is the workspace divided into crates and runtime layers? | New contributors and maintainers |
| [`02-data-model.md`](02-data-model.md) | Which domain, configuration, and persistent-state types does the application use? | Core, UI, and integration contributors |
| [`03-emby-api.md`](03-emby-api.md) | How does the Emby client authenticate, query, stream, and report playback? | Emby-client and network contributors |
| [`04-state-and-input.md`](04-state-and-input.md) | How do actions, events, effects, reducers, and key bindings move through the application? | Core, player, and UI contributors |
| [`05-audio-engine.md`](05-audio-engine.md) | How does the audio backend communicate with libmpv and expose playback features? | Audio contributors |
| [`06-cache-and-offline.md`](06-cache-and-offline.md) | How does local storage support cache, downloads, offline browsing, and deferred work? | Cache and player contributors |
| [`07-ui-spec.md`](07-ui-spec.md) | How do the terminal UI, views, modals, widgets, and themes fit together? | UI contributors and designers |
| [`09-traceability.md`](09-traceability.md) | Where in the source tree does each system concern live? | Maintainers navigating the codebase |
| [`10-testing-and-ci.md`](10-testing-and-ci.md) | Which checks protect the workspace locally and in CI? | Contributors and release maintainers |
| [`11-packaging.md`](11-packaging.md) | How is the application distributed and diagnosed on supported platforms? | Release and packaging maintainers |
| [`14-manual-test-plan.md`](14-manual-test-plan.md) | Which user-visible behaviours require manual verification? | Testers and release maintainers |

## Reference

| Filename | Question answered | Who should read it |
| --- | --- | --- |
| [`12-decisions.md`](12-decisions.md) | Which implementation decisions and documentation corrections shape the current system? | Maintainers and contributors changing established behaviour |
| [`13-dependencies.md`](13-dependencies.md) | Which dependencies are allowed and why are they present? | Contributors changing `Cargo.toml` files |

## Planning

| Filename | Question answered | Who should read it |
| --- | --- | --- |
| [`ROADMAP.md`](ROADMAP.md) | Which capabilities are intentionally not present yet? | Maintainers planning future work |
