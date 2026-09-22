# Documentation

`docs/` describes the system as it is; planned work lives in [`ROADMAP.md`](ROADMAP.md).

## Design & architecture

| Filename | Question it answers | Who should read it |
|---|---|---|
| [`01-architecture.md`](01-architecture.md) | How are the workspace crates and runtime layers arranged? | New contributors |
| [`02-data-model.md`](02-data-model.md) | Which domain types, configuration files, and paths define loxia's data? | Core and integration contributors |
| [`03-emby-api.md`](03-emby-api.md) | How does loxia communicate with an Emby server? | Network and Emby contributors |
| [`04-state-and-input.md`](04-state-and-input.md) | How do actions, state, reducers, effects, and key input interact? | Core and UI contributors |
| [`05-audio-engine.md`](05-audio-engine.md) | How does the playback engine use libmpv? | Audio contributors |
| [`06-cache-and-offline.md`](06-cache-and-offline.md) | How does persistent cache and offline data work? | Cache and runtime contributors |
| [`07-ui-spec.md`](07-ui-spec.md) | How does the ratatui interface map application state to terminal views? | UI contributors |
| [`10-testing-and-ci.md`](10-testing-and-ci.md) | Which automated checks protect the workspace? | All contributors |
| [`11-packaging.md`](11-packaging.md) | How are releases, licences, and runtime dependencies handled? | Release maintainers |
| [`14-manual-test-plan.md`](14-manual-test-plan.md) | Which user-facing workflows need manual verification? | Testers and release maintainers |

## Reference

| Filename | Question it answers | Who should read it |
|---|---|---|
| [`09-traceability.md`](09-traceability.md) | Where does a system concern live in the source tree? | Contributors locating code |
| [`12-decisions.md`](12-decisions.md) | Why does the implementation make its recorded design decisions? | Contributors changing behaviour or dependencies |
| [`13-dependencies.md`](13-dependencies.md) | Which dependencies does the workspace use and what rules govern them? | Contributors changing `Cargo.toml` files |
| [`README.md`](README.md) | Where should a reader start in this reference set? | Everyone |

## Planning

| Filename | Question it answers | Who should read it |
|---|---|---|
| [`ROADMAP.md`](ROADMAP.md) | What work remains outside the current implementation? | Maintainers and contributors choosing future work |
