# loxia reference documentation

`docs/` describes the system as it is; planned work lives in [`ROADMAP.md`](ROADMAP.md).

## Design & architecture

| File | Question it answers | Who should read it |
| :-- | :-- | :-- |
| [`01-architecture.md`](01-architecture.md) | How are the workspace crates and runtime layers arranged? | Everyone new to the codebase |
| [`02-data-model.md`](02-data-model.md) | Which domain, configuration, and persisted-state types exist? | Contributors changing core data or configuration |
| [`03-emby-api.md`](03-emby-api.md) | How does the Emby client communicate with a server? | Contributors changing network behaviour |
| [`04-state-and-input.md`](04-state-and-input.md) | How do actions, key bindings, state, and reducers interact? | Contributors changing interaction behaviour |
| [`05-audio-engine.md`](05-audio-engine.md) | How does playback and audio configuration work? | Contributors changing audio behaviour |
| [`06-cache-and-offline.md`](06-cache-and-offline.md) | How are cached media, downloads, and offline state managed? | Contributors changing local persistence |
| [`07-ui-spec.md`](07-ui-spec.md) | How does the terminal interface present application state? | Contributors changing the TUI |
| [`09-traceability.md`](09-traceability.md) | Where does a system concern live in the workspace? | Reviewers and contributors locating code |

## Reference

| File | Question it answers | Who should read it |
| :-- | :-- | :-- |
| [`10-testing-and-ci.md`](10-testing-and-ci.md) | Which checks protect the workspace and how are they run? | Contributors and maintainers |
| [`11-packaging.md`](11-packaging.md) | How is loxia distributed and diagnosed on supported platforms? | Release maintainers |
| [`12-decisions.md`](12-decisions.md) | Which significant implementation decisions and behaviour changes are recorded? | Contributors making comparable changes |
| [`13-dependencies.md`](13-dependencies.md) | Which dependencies does the workspace use and what rules govern them? | Contributors changing dependencies |
| [`14-manual-test-plan.md`](14-manual-test-plan.md) | Which user-facing behaviours require manual verification? | Testers and release maintainers |
| [`README.md`](README.md) | Where should a reader start in this reference set? | Everyone |

## Planning

| File | Question it answers | Who should read it |
| :-- | :-- | :-- |
| [`ROADMAP.md`](ROADMAP.md) | Which work remains planned rather than implemented? | Contributors planning future work |
