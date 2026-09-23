# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Workspace scaffolding: six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`,
  `loxia-cache`, `loxia-tui`, `loxia-player`) with the full module tree per `docs/01-architecture.md`.
- Locked dependency set per `docs/13-dependencies.md`.

### Notes (hand-back, not a fix)
- The five `loxia-tui` snapshot test failures (`render::tests::layout_snapshot_80x24`,
  `render::tests::layout_snapshot_120x30`, `render::tests::layout_snapshot_200x50`,
  `views::now_playing::tests::now_playing_snapshot_history`,
  `widgets::header::tests::header_snapshot_offline_with_downloads`) were **not** fixed in this
  change. The session that produced this commit had no shell/tool access and no visibility into
  the current contents of `crates/loxia-tui/src/render.rs`,
  `crates/loxia-tui/src/widgets/header.rs`, `crates/loxia-tui/src/views/now_playing.rs`, the five
  `.snap` files under test, or `crates/loxia-core/src/keymap/*` — so it was not possible to run
  `cargo insta test -p loxia-tui --review`, `git log --follow -- <snap>`, or
  `git log <commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` as Steps 1–2 of
  the task require, and therefore not possible to produce a real, citable (a)/(b)/(c) verdict
  with an actual commit hash for any of the five tests.
- No source or snapshot file under `crates/loxia-tui` was modified, to avoid overwriting real,
  working rendering code with a fabricated guess at its contents.
- This should be re-run in a session with working-tree and `git log` access to
  `crates/loxia-tui` and `crates/loxia-core/src/keymap*` so the verdicts can be backed by real
  `cargo insta` diffs and real commit hashes, per the task's own Step 1–4.
