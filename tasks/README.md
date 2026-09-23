# Task library

This is the master index the workflow in `CONTRIBUTING.md` refers to: pick the lowest-numbered
unticked task below whose prerequisites are already ticked. Each task file under `tasks/phase-*/`
is self-contained. Tick a box in the same PR that completes the task.

## Phase 00 — Scaffolding
- [x] phase-00-scaffolding/00-01-workspace-skeleton.md
- [x] phase-00-scaffolding/00-02-workspace-dependencies.md
- [x] phase-00-scaffolding/00-03-dev-tooling-and-licence.md
- [x] phase-00-scaffolding/00-04-ci-workflow.md
- [x] phase-00-scaffolding/00-05-cargo-deny-policy.md

## Phase 01 — Config
- [x] phase-01-config/01-01-config-schema.md
- [x] phase-01-config/01-02-config-defaults-and-validation.md
- [x] phase-01-config/01-03-path-resolution.md
- [x] phase-01-config/01-04-config-file-io.md
- [x] phase-01-config/01-05-terminal-guard.md
- [x] phase-01-config/01-06-cli-and-logging.md
- [x] phase-01-config/01-07-domain-model-types.md
- [x] phase-01-config/01-08-lyrics-model-and-lrc-parser.md

## Phase 02 — Emby client
- [x] phase-02-emby-client/02-01-api-audit.md
- [x] phase-02-emby-client/02-02-http-client-and-auth.md
- [x] phase-02-emby-client/02-03-errors-and-retry.md
- [x] phase-02-emby-client/02-04-dtos-and-conversion.md
- [x] phase-02-emby-client/02-05-item-query-builder.md
- [x] phase-02-emby-client/02-06-discography-appears-on.md
- [x] phase-02-emby-client/02-07-search-favourites-instant-mix.md
- [x] phase-02-emby-client/02-08-playlists.md
- [x] phase-02-emby-client/02-09-playbackinfo-and-stream-urls.md
- [x] phase-02-emby-client/02-10-playback-reporting.md
- [x] phase-02-emby-client/02-11-lyrics.md
- [x] phase-02-emby-client/02-12-images.md
- [x] phase-02-emby-client/02-13-probe-example.md

## Phase 03 — State machine
- [x] phase-03-state-machine/03-01-appstate-and-substates.md
- [x] phase-03-state-machine/03-02-test-support-fixtures.md
- [x] phase-03-state-machine/03-03-action-effect-event.md
- [x] phase-03-state-machine/03-04-key-chords-and-parser.md
- [x] phase-03-state-machine/03-05-default-keymap-and-validation.md
- [x] phase-03-state-machine/03-06-reducer-navigation.md
- [x] phase-03-state-machine/03-07-reducer-modals.md
- [x] phase-03-state-machine/03-08-runtime-event-loop.md
- [x] phase-03-state-machine/03-09-input-mapping.md

## Phase 04 — Miller-columns UI
- [x] phase-04-miller-ui/04-01-theme-system.md
- [x] phase-04-miller-ui/04-02-root-layout.md
- [x] phase-04-miller-ui/04-03-text-helpers.md
- [x] phase-04-miller-ui/04-04-hit-map.md
- [x] phase-04-miller-ui/04-05-sidebar-and-header.md
- [x] phase-04-miller-ui/04-06-column-widget.md
- [x] phase-04-miller-ui/04-07-miller-view.md
- [x] phase-04-miller-ui/04-08-inspector.md
- [x] phase-04-miller-ui/04-09-player-bar.md
- [x] phase-04-miller-ui/04-10-network-worker-and-wiring.md
- [x] phase-04-miller-ui/04-11-inline-filter.md

## Phase 05 — Audio
- [x] phase-05-audio/05-01-backend-trait-and-types.md
- [x] phase-05-audio/05-02-mock-engine.md
- [x] phase-05-audio/05-03-mpv-handle.md
- [x] phase-05-audio/05-04-mpv-event-pump.md
- [x] phase-05-audio/05-05-custom-headers-and-diagnostics.md
- [x] phase-05-audio/05-06-audio-worker.md

## Phase 06 — Queue
- [x] phase-06-queue/06-01-queue-state-basics.md
- [x] phase-06-queue/06-02-appears-on-queue-rules.md
- [x] phase-06-queue/06-03-shuffle.md
- [x] phase-06-queue/06-04-sort-profiles.md
- [x] phase-06-queue/06-05-listening-history.md
- [x] phase-06-queue/06-06-gapless-preloading.md
- [x] phase-06-queue/06-07-playback-reporting-wiring.md
- [x] phase-06-queue/06-08-instant-mix.md

## Phase 07 — Views
- [x] phase-07-views/07-01-search-tab.md
- [x] phase-07-views/07-02-favourites-tab.md
- [x] phase-07-views/07-03-playlists-tab.md
- [x] phase-07-views/07-04-genres-tab.md
- [x] phase-07-views/07-05-folders-tab.md
- [x] phase-07-views/07-06-now-playing-view.md
- [x] phase-07-views/07-07-lyrics-pane.md

`07-08-queue-view-play-order-audit.md` has been deleted: `docs/15-play-order-consumer-audit.md`
confirmed both `views/now_playing.rs` and `widgets/player_bar.rs` already read the current/next
track through `play_order`, so there was nothing to hand off or fix.

## Phase 08 — Cache and offline
- [x] phase-08-cache-offline/08-01-cache-paths-and-sanitiser.md
- [x] phase-08-cache-offline/08-02-manifest-and-lru.md
- [x] phase-08-cache-offline/08-03-cache-write-through.md
- [x] phase-08-cache-offline/08-04-permanent-downloads.md
- [x] phase-08-cache-offline/08-05-offline-browse-index.md
- [x] phase-08-cache-offline/08-06-connectivity-state-machine.md
- [x] phase-08-cache-offline/08-07-scrobble-buffer.md
- [x] phase-08-cache-offline/08-08-session-and-history-persistence.md

## Phase 09 — Advanced audio
- [x] phase-09-advanced-audio/09-01-device-enumeration-and-swap.md
- [x] phase-09-advanced-audio/09-02-bit-perfect-mode.md
- [x] phase-09-advanced-audio/09-03-equalizer-engine.md
- [x] phase-09-advanced-audio/09-04-replay-gain.md
- [x] phase-09-advanced-audio/09-05-sleep-timer.md
- [x] phase-09-advanced-audio/09-06-quality-profiles.md

## Phase 10 — Polish
- [x] phase-10-polish/10-01-album-art.md
- [x] phase-10-polish/10-02-zen-mode.md
- [x] phase-10-polish/10-03-help-modal.md
- [x] phase-10-polish/10-04-mouse-support.md
- [x] phase-10-polish/10-05-device-picker-modal.md
- [x] phase-10-polish/10-06-equalizer-modal.md
- [x] phase-10-polish/10-07-sleep-timer-modal.md
- [x] phase-10-polish/10-08-save-playlist-modal.md
- [x] phase-10-polish/10-09-sort-profile-modal.md
- [x] phase-10-polish/10-10-desktop-notifications.md
- [x] phase-10-polish/10-11-media-keys.md
- [x] phase-10-polish/10-12-websocket-remote-control.md
- [x] phase-10-polish/10-13-toasts-and-empty-states.md
- [ ] phase-10-polish/10-14-mpris-play-order-audit.md — **new, follow-up from the play-order
      consumer audit** (see below)

## Phase 11 — Settings
- [x] phase-11-settings/11-01-settings-view.md
- [x] phase-11-settings/11-02-keymap-editor.md
- [x] phase-11-settings/11-03-server-profiles.md
- [x] phase-11-settings/11-04-sort-profile-editor.md
- [x] phase-11-settings/11-05-eq-preset-manager.md
- [x] phase-11-settings/11-06-session-restore-wiring.md
- [x] phase-11-settings/11-07-about-view.md

## Phase 12 — Packaging
- [x] phase-12-packaging/12-01-cargo-dist-setup.md
- [x] phase-12-packaging/12-02-windows-packaging.md
- [x] phase-12-packaging/12-03-macos-packaging.md
- [x] phase-12-packaging/12-04-linux-packaging.md
- [x] phase-12-packaging/12-05-licence-compliance-checks.md
- [x] phase-12-packaging/12-06-branding-assets.md
- [x] phase-12-packaging/12-07-readme-and-user-docs.md
- [x] phase-12-packaging/12-08-doctor-subcommand.md

## Follow-up / audit tasks

Work items outside the numbered phase sequence (audits, cross-cutting checks) register their
findings here rather than getting their own phase number.

- [ ] **`phase-10-polish/10-14-mpris-play-order-audit.md`** — `loxia-player`, single-crate fix.
  `crates/loxia-player/src/workers/mpris.rs::build_track_list` iterates `QueueState.entries` in
  storage order instead of resolving through `QueueState.play_order`, so the MPRIS `TrackList`
  interface is wrong under shuffle. Found by the play-order consumer audit
  (`docs/15-play-order-consumer-audit.md`); current track, `CanGoNext`, and `CanGoPrevious` in the
  same file were all confirmed correct and need no change.

No other follow-up tasks were registered by that audit: `PlaybackReport` construction
(`loxia-core`), the `loxia-tui` queue view, and the `loxia-tui` player bar were all confirmed to
already read through `play_order` correctly. See `docs/15-play-order-consumer-audit.md` for the
full per-site table.
