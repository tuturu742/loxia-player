# Documentation index

`docs/` describes the system as it is; planned work lives in [`ROADMAP.md`](ROADMAP.md).

## Design & architecture

| Filename | Question it answers | Who should read it |
|---|---|---|
| [`01-architecture.md`](01-architecture.md) | How are the workspace crates and runtime boundaries arranged? | New contributors |
| [`02-data-model.md`](02-data-model.md) | Which domain models, configuration data, and paths does loxia use? | Contributors working in `loxia-core` |
| [`03-emby-api.md`](03-emby-api.md) | How does loxia communicate with an Emby server? | Contributors working on network features |
| [`04-state-and-input.md`](04-state-and-input.md) | How do actions, reducers, effects, state, and key input interact? | Contributors working on application behaviour |
| [`05-audio-engine.md`](05-audio-engine.md) | How does the playback layer control libmpv? | Contributors working on audio |
| [`06-cache-and-offline.md`](06-cache-and-offline.md) | How are local cache, downloads, offline data, and persisted state organised? | Contributors working on storage or offline behaviour |
| [`07-ui-spec.md`](07-ui-spec.md) | How is the terminal interface structured and rendered? | Contributors working on `loxia-tui` |
| [`09-traceability.md`](09-traceability.md) | Where is each system responsibility implemented and tested? | Reviewers and maintainers |
| [`10-testing-and-ci.md`](10-testing-and-ci.md) | Which checks protect the workspace and how are they run? | Contributors and CI maintainers |
| [`11-packaging.md`](11-packaging.md) | What does the current distribution and licensing surface contain? | Release and distribution maintainers |
| [`14-manual-test-plan.md`](14-manual-test-plan.md) | Which user-facing behaviours require manual verification? | Testers and release maintainers |

## Reference

| Filename | Question it answers | Who should read it |
|---|---|---|
| [`12-decisions.md`](12-decisions.md) | Why do documented implementation choices differ from earlier designs or upstream assumptions? | Contributors changing behaviour or dependencies |
| [`13-dependencies.md`](13-dependencies.md) | Which dependencies are approved and what constraints apply to them? | Contributors changing `Cargo.toml` files |

## Planning

| Filename | Question it answers | Who should read it |
|---|---|---|
| [`ROADMAP.md`](ROADMAP.md) | What work is not part of the current system? | Maintainers and contributors planning future work |
