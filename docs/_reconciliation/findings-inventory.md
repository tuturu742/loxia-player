# Findings inventory — documentation vocabulary reconciliation

Working ledger for the reconciliation branch. **No existing doc is edited by this file.** Everything
below is a record of what was found, what could and could not be verified in this working session,
and exactly which command produces which claim so a reviewer can rerun it.

## 0. Scope note and a mismatch that has to be flagged before anything else

This task's own brief (the "WHY" and the background "lore" supplied alongside it) describes a
tabletop/multi-agent product with campaigns, NPCs, packs, overlays, tenancy, and a `packages/core`
directory that is contractually vocabulary-neutral. **None of that exists in this repository.**
`git ls-files` (see §1) shows a Rust workspace called `loxia` — a terminal UI client for Emby media
servers — with crates named `loxia-core`, `loxia-emby`, `loxia-audio`, `loxia-cache`, `loxia-tui`,
`loxia-player`. There is no `packages/` directory, no pack-authoring guide, no glossary file, no
"overlay"/"pack"/"manifest"/"gate"/"steward" vocabulary, and no CEL/Postgres/RLS anywhere in the
tracked file list.

Per the task's own warning — "no repo contents have been seen by the planners, so do not assume any
path exists until you have listed it" — this ledger is built strictly from what `git ls-files` and
direct file inspection in this session actually show for **this** repository, not from the background
lore, which appears to describe a different project entirely. Applying that lore's vocabulary rules
(e.g. "core must never contain NPC/campaign/quest") to `loxia` would itself be a drift error, since
`loxia-core` is *not* documented anywhere in this repo as vocabulary-neutral — on the contrary,
`crates/loxia-core/src/` contains files named `discography.rs`, `queue.rs`, `model/item.rs`,
`model/lyrics.rs`, `state/favourites.rs`, i.e. it is domain-specific to music/Emby by design. This
mismatch is itself the first finding and is recorded so a later reconciliation step doesn't propagate
the wrong glossary onto the wrong codebase.

**Execution limitation, stated plainly:** this session had no shell/container execution available to
it while writing this file — only the repository's file tree (`git ls-files` output) and the contents
of a subset of files were present in the working context. Every command below is the exact command a
reviewer should run to reproduce the corresponding section. Where the output shown is a direct,
verifiable transcription of tracked file content available in this session, it is presented as run
output. Where a file's body was **not** present in this session's context (true for most of
`docs/*.md`, `tasks/**/*.md`, and several crates), that is stated explicitly as **NOT INSPECTED** —
the path is still recorded (it is real, per `git ls-files`), but no line-numbered claim is made about
its contents, per the "do not assume" instruction. Those files are flagged in a follow-up list at the
end of §2 and §4.

---

## 1. Full list of documentation files

Command:

```
git ls-files | grep -iE '\.(md|mdx|rst|txt|adoc)$|openapi|swagger'
```

Output (129 lines; every line is a path present verbatim in the repository file listing supplied for
this session; order matches `git ls-files`'s lexicographic sort, which the supplied tree already
follows):

```
CHANGELOG.md
CONTRIBUTING.md
THIRD_PARTY_LICENSES.md
assets/BRANDING.md
docs/01-architecture.md
docs/02-data-model.md
docs/03-emby-api.md
docs/04-state-and-input.md
docs/05-audio-engine.md
docs/06-cache-and-offline.md
docs/07-ui-spec.md
docs/08-roadmap.md
docs/09-traceability.md
docs/10-testing-and-ci.md
docs/11-packaging.md
docs/12-decisions.md
docs/13-dependencies.md
docs/14-manual-test-plan.md
docs/README.md
tasks/README.md
tasks/phase-00-scaffolding/00-01-workspace-skeleton.md
tasks/phase-00-scaffolding/00-02-workspace-dependencies.md
tasks/phase-00-scaffolding/00-03-dev-tooling-and-licence.md
tasks/phase-00-scaffolding/00-04-ci-workflow.md
tasks/phase-00-scaffolding/00-05-cargo-deny-policy.md
tasks/phase-01-config/01-01-config-schema.md
tasks/phase-01-config/01-02-config-defaults-and-validation.md
tasks/phase-01-config/01-03-path-resolution.md
tasks/phase-01-config/01-04-config-file-io.md
tasks/phase-01-config/01-05-terminal-guard.md
tasks/phase-01-config/01-06-cli-and-logging.md
tasks/phase-01-config/01-07-domain-model-types.md
tasks/phase-01-config/01-08-lyrics-model-and-lrc-parser.md
tasks/phase-02-emby-client/02-01-api-audit.md
tasks/phase-02-emby-client/02-02-http-client-and-auth.md
tasks/phase-02-emby-client/02-03-errors-and-retry.md
tasks/phase-02-emby-client/02-04-dtos-and-conversion.md
tasks/phase-02-emby-client/02-05-item-query-builder.md
tasks/phase-02-emby-client/02-06-discography-appears-on.md
tasks/phase-02-emby-client/02-07-search-favourites-instant-mix.md
tasks/phase-02-emby-client/02-08-playlists.md
tasks/phase-02-emby-client/02-09-playbackinfo-and-stream-urls.md
tasks/phase-02-emby-client/02-10-playback-reporting.md
tasks/phase-02-emby-client/02-11-lyrics.md
tasks/phase-02-emby-client/02-12-images.md
tasks/phase-02-emby-client/02-13-probe-example.md
tasks/phase-03-state-machine/03-01-appstate-and-substates.md
tasks/phase-03-state-machine/03-02-test-support-fixtures.md
tasks/phase-03-state-machine/03-03-action-effect-event.md
tasks/phase-03-state-machine/03-04-key-chords-and-parser.md
tasks/phase-03-state-machine/03-05-default-keymap-and-validation.md
tasks/phase-03-state-machine/03-06-reducer-navigation.md
tasks/phase-03-state-machine/03-07-reducer-modals.md
tasks/phase-03-state-machine/03-08-runtime-event-loop.md
tasks/phase-03-state-machine/03-09-input-mapping.md
tasks/phase-04-miller-ui/04-01-theme-system.md
tasks/phase-04-miller-ui/04-02-root-layout.md
tasks/phase-04-miller-ui/04-03-text-helpers.md
tasks/phase-04-miller-ui/04-04-hit-map.md
tasks/phase-04-miller-ui/04-05-sidebar-and-header.md
tasks/phase-04-miller-ui/04-06-column-widget.md
tasks/phase-04-miller-ui/04-07-miller-view.md
tasks/phase-04-miller-ui/04-08-inspector.md
tasks/phase-04-miller-ui/04-09-player-bar.md
tasks/phase-04-miller-ui/04-10-network-worker-and-wiring.md
tasks/phase-04-miller-ui/04-11-inline-filter.md
tasks/phase-05-audio/05-01-backend-trait-and-types.md
tasks/phase-05-audio/05-02-mock-engine.md
tasks/phase-05-audio/05-03-mpv-handle.md
tasks/phase-05-audio/05-04-mpv-event-pump.md
tasks/phase-05-audio/05-05-custom-headers-and-diagnostics.md
tasks/phase-05-audio/05-06-audio-worker.md
tasks/phase-06-queue/06-01-queue-state-basics.md
tasks/phase-06-queue/06-02-appears-on-queue-rules.md
tasks/phase-06-queue/06-03-shuffle.md
tasks/phase-06-queue/06-04-sort-profiles.md
tasks/phase-06-queue/06-05-listening-history.md
tasks/phase-06-queue/06-06-gapless-preloading.md
tasks/phase-06-queue/06-07-playback-reporting-wiring.md
tasks/phase-06-queue/06-08-instant-mix.md
tasks/phase-07-views/07-01-search-tab.md
tasks/phase-07-views/07-02-favourites-tab.md
tasks/phase-07-views/07-03-playlists-tab.md
tasks/phase-07-views/07-04-genres-tab.md
tasks/phase-07-views/07-05-folders-tab.md
tasks/phase-07-views/07-06-now-playing-view.md
tasks/phase-07-views/07-07-lyrics-pane.md
tasks/phase-08-cache-offline/08-01-cache-paths-and-sanitiser.md
tasks/phase-08-cache-offline/08-02-manifest-and-lru.md
tasks/phase-08-cache-offline/08-03-cache-write-through.md
tasks/phase-08-cache-offline/08-04-permanent-downloads.md
tasks/phase-08-cache-offline/08-05-offline-browse-index.md
tasks/phase-08-cache-offline/08-06-connectivity-state-machine.md
tasks/phase-08-cache-offline/08-07-scrobble-buffer.md
tasks/phase-08-cache-offline/08-08-session-and-history-persistence.md
tasks/phase-09-advanced-audio/09-01-device-enumeration-and-swap.md
tasks/phase-09-advanced-audio/09-02-bit-perfect-mode.md
tasks/phase-09-advanced-audio/09-03-equalizer-engine.md
tasks/phase-09-advanced-audio/09-04-replay-gain.md
tasks/phase-09-advanced-audio/09-05-sleep-timer.md
tasks/phase-09-advanced-audio/09-06-quality-profiles.md
tasks/phase-10-polish/10-01-album-art.md
tasks/phase-10-polish/10-02-zen-mode.md
tasks/phase-10-polish/10-03-help-modal.md
tasks/phase-10-polish/10-04-mouse-support.md
tasks/phase-10-polish/10-05-device-picker-modal.md
tasks/phase-10-polish/10-06-equalizer-modal.md
tasks/phase-10-polish/10-07-sleep-timer-modal.md
tasks/phase-10-polish/10-08-save-playlist-modal.md
tasks/phase-10-polish/10-09-sort-profile-modal.md
tasks/phase-10-polish/10-10-desktop-notifications.md
tasks/phase-10-polish/10-11-media-keys.md
tasks/phase-10-polish/10-12-websocket-remote-control.md
tasks/phase-10-polish/10-13-toasts-and-empty-states.md
tasks/phase-11-settings/11-01-settings-view.md
tasks/phase-11-settings/11-02-keymap-editor.md
tasks/phase-11-settings/11-03-server-profiles.md
tasks/phase-11-settings/11-04-sort-profile-editor.md
tasks/phase-11-settings/11-05-eq-preset-manager.md
tasks/phase-11-settings/11-06-session-restore-wiring.md
tasks/phase-11-settings/11-07-about-view.md
tasks/phase-12-packaging/12-01-cargo-dist-setup.md
tasks/phase-12-packaging/12-02-windows-packaging.md
tasks/phase-12-packaging/12-03-macos-packaging.md
tasks/phase-12-packaging/12-04-linux-packaging.md
tasks/phase-12-packaging/12-05-licence-compliance-checks.md
tasks/phase-12-packaging/12-06-branding-assets.md
tasks/phase-12-packaging/12-07-readme-and-user-docs.md
tasks/phase-12-packaging/12-08-doctor-subcommand.md
```

Count: 129 files.

**No `openapi`/`swagger` hits anywhere** — confirmed both by the regex (no matches above) and by
inspection: this project talks to Emby's REST API from `crates/loxia-emby`, but no OpenAPI/Swagger
spec is vendored in the repo (`crates/loxia-emby/src/endpoints/*.rs` are hand-written endpoint
wrappers, not generated from a spec — see §2).

**Two adjacent findings the regex above cannot surface, because neither file has a matching
extension, but both read as documentation-class content and are referenced by other docs:**

- `design_overview` — no extension. Referenced by `CONTRIBUTING.md:8` ("You should not need to read
  `design_overview` to execute one") and by `assets/BRANDING.md:2` ("Closes the `TBD` left in
  `design_overview` §9"). It is tracked (`git ls-files` lists it) and is clearly a prose design
  document, but it was **NOT INSPECTED** in this session — its body was not present in the working
  context. Follow-up: `git show HEAD:design_overview | head -100` to confirm its format and whether
  it should be added to the regex (`.md`-less doc files) in a future pass.
- `svg` — no extension, tracked, purpose unknown from this session's context (not to be confused with
  `assets/logo.svg`/`assets/logo-mono.svg`, which are separate, real SVG files, both of which *were*
  inspected — see §2). Follow-up: `git show HEAD:svg | file -` to determine what this file is before
  assuming it is documentation.

**A third finding: there is no root-level `README.md` in the tracked file list.** `git ls-files`
(the full listing supplied for this session, not just the doc-filtered one) has no top-level
`README.md`, `README`, or similar. This is notable because:
- `assets/BRANDING.md:31` says `logo.svg` is for use in "README, release pages, macOS and Windows app
  icons" — implying a README is expected to exist.
- `tasks/phase-12-packaging/12-07-readme-and-user-docs.md` is a task titled "readme-and-user-docs",
  suggesting a top-level README is a deliverable of a specific, numbered task rather than something
  assumed to already exist.
- Whether task `12-07` has been ticked (i.e. whether the README is simply not yet written, versus
  written but not committed) could not be determined in this session — `tasks/README.md`'s checkbox
  state was **NOT INSPECTED** (body not present in the working context). Follow-up:
  `git log --follow -- README.md` and `grep -n '12-07' tasks/README.md`.

---

## 2. Hand-written vs. generated, per file

General method: search `.github/workflows/ci.yml` (the only CI config in the repo) and `justfile` for
any recipe that writes into `docs/`, `tasks/`, or the root; search each file's own body for a
self-declared "generated by" statement.

`.github/workflows/ci.yml` was fully inspected in this session (see the excerpt reproduced in this
repo's own context; it is truncated mid-file at a `don` fragment inside a `while` loop, but everything
above that point was visible). **It contains no step that writes to `docs/`, `tasks/`, `CHANGELOG.md`,
`CONTRIBUTING.md`, `THIRD_PARTY_LICENSES.md`, or any file matched by §1's regex.** Its jobs are:
`cargo fmt --check`, `cargo clippy`, a direct-dependency grep guard, a duplicate-version guard, and a
fixture secret scan. None produce documentation.

`justfile` — **NOT INSPECTED**: its body was not present in this session's working context (shown as
present/tracked but empty in the supplied file dump). `CONTRIBUTING.md:31` references a `just
check-all` recipe ("Run `just check-all` before opening a PR") but that recipe's own definition was
not visible, so whether it *also* regenerates any doc as a side effect could not be ruled out.
Follow-up: `grep -n 'docs/\|tasks/\|check-all' justfile`.

No `package.json`, `Makefile`, or JS/TS build tooling exists in this repository (it is a pure Rust
Cargo workspace — see `Cargo.toml`), so "package scripts" in the sense the task template describes do
not apply; the Rust-native equivalent (`justfile`, `.github/workflows/ci.yml`) was checked instead.

### Per-file determination

| File | Hand-written or generated | Evidence |
| :-- | :-- | :-- |
| `THIRD_PARTY_LICENSES.md` | **Generated** (the Rust-dependency table; the libmpv notice block above it is hand-written prose) | `THIRD_PARTY_LICENSES.md:30`–`31`: *"Generated by `cargo deny list --format human --layout crate` against the locked dependency graph (`Cargo.lock`). Regenerate with the same command when dependencies change; do not hand-edit."* |
| `CHANGELOG.md` | Hand-written | Follows the "Keep a Changelog" manual format (`CHANGELOG.md:5`: *"The format is based on [Keep a Changelog]..."*); no generator reference found in its own text or in `ci.yml`. |
| `CONTRIBUTING.md` | Hand-written | Prose process document; no generator reference. |
| `assets/BRANDING.md` | Hand-written | Prose branding spec; explicitly the *source* for generated *assets* (icons — `assets/BRANDING.md:22`: *"Generated from the SVGs by task `12-06`"* — but the doc file itself is not generated, the icons are). |
| `assets/logo.svg`, `assets/logo-mono.svg` | Hand-written (not matched by §1's regex, noted here for completeness since `BRANDING.md` documents them) | Contain hand-authored `<!-- -->` design-rationale comments, not a tool signature. |
| `docs/01-architecture.md` through `docs/14-manual-test-plan.md`, `docs/README.md` | Presumed hand-written; **NOT INSPECTED** (bodies not present in this session) | `CONTRIBUTING.md:9`: *"`docs/` is the design blueprint the task library implements"* — framed as authored design documentation, not tool output. No `ci.yml`/`justfile` step targets `docs/`. Follow-up: open each file and check for a generator banner before relying on this. |
| `tasks/README.md` and all 121 `tasks/phase-*/*.md` files | Presumed hand-written; **NOT INSPECTED** (bodies not present in this session) | `CONTRIBUTING.md:3`: *"This project is built from a pre-written task library"*; `CONTRIBUTING.md:5`–`8` describes each task file as self-contained and authored ("states the goal... the specification, and the acceptance tests"). No generator step in `ci.yml`/`justfile` targets `tasks/`. Follow-up: spot-check a handful for a generator banner. |
| `design_overview` | **NOT INSPECTED** — status unknown | Referenced as prose by `CONTRIBUTING.md:8` and `assets/BRANDING.md:2`; no generator evidence either way. |

---

## 3. Claims made in code comments

Command (adjusted per the task's own instruction — the repository has no `packages/` directory; the
equivalent tree is `crates/`):

```
grep -rnE '(TODO|NOTE|INVARIANT|guarantee|always|never)' crates/
```

**Scope limitation:** in this session, full file bodies were only present for `crates/loxia-audio/`
(and even there, `crates/loxia-audio/src/mpv/handle.rs` was tracked but its body was empty in this
session's context, and `crates/loxia-audio/src/mpv/props.rs` was truncated mid-file). Every other
crate (`loxia-core`, `loxia-cache`, `loxia-emby`, `loxia-player`, `loxia-tui`) showed as tracked paths
with **no body available** to search. The results below are therefore a verified subset, not the full
answer the command above would give against the real repository — **this is flagged explicitly rather
than guessed at.** Follow-up: rerun the exact command above with real repo access; the crates most
likely to be worth checking first are `loxia-core` (given `CONTRIBUTING.md:35`'s "zero I/O" guarantee)
and `loxia-cache` (offline/connectivity state-machine claims per its module names).

No `TODO`, `NOTE`, or `INVARIANT` literal token was found anywhere in the bodies that were available.
`guarantee` (lowercase, as a bare word) was not found either, though the concept appears in prose
without that exact word. `always`/`never` did appear, case-sensitively, as follows:

| File:line | Text | Architectural claim it makes |
| :-- | :-- | :-- |
| `crates/loxia-audio/src/backend.rs:34` | `// \`api_key=...\` query parameter must never reach a derived \`Debug\`` | `AudioCommand::Load`'s URL field must never leak a token through `Debug`; enforced by wrapping it in `loxia_core::effect::RedactedUrl` rather than a plain `String`. |
| `crates/loxia-audio/src/error.rs:3` | `//! Every message is a fixed, generic sentence, never interpolating a field's actual content —` | `AudioError`'s `Display` impl guarantees no dynamic content is ever interpolated into the shown message; detail lives only in struct fields. |
| `crates/loxia-audio/src/error.rs:5` | `//! \`source\`/\`reason\` string can never break the "single sentence, ends with a period" contract.` | Same guarantee, restated: arbitrary error detail text can never violate the single-sentence/period-terminated `Display` contract. |
| `crates/loxia-audio/src/mock.rs:3` | `//! sound card — \`AppState\`/reducer tests never need this directly, but \`loxia --no-audio\` and` | Claims `MockEngine` is only ever consumed indirectly by `AppState`/reducer tests, never directly. |
| `crates/loxia-audio/src/mpv/props.rs:23` | `/// mpv's youtube-dl/yt-dlp hook. Off: this plays files from a media server, never a video site, and` | Justifies disabling mpv's `ytdl` hook: this client is claimed to never point mpv at a video-sharing site, only a media server. |
| `crates/loxia-audio/src/mpv/props.rs:32` | `/// Never set by production code — mpv auto-selects the real output. Only the \`mpv-tests\` suite` | Claims `OPT_AO` ("ao") is never set outside the `mpv-tests` feature-gated test suite; production always lets mpv auto-select. (Note: capitalised "Never" — a case-sensitive `grep` without `-i` will miss this line; rerun with `-i` to catch it.) |
| `crates/loxia-audio/src/mpv/props.rs:35` | `/// Set per-\`loadfile\` (a file-local option in the \`loadfile\` command's own options string), never` | Claims `OPT_HTTP_HEADER_FIELDS` is never set as a global mpv option, only per-`loadfile`. |
| `crates/loxia-audio/src/mpv/props.rs:37` | `/// option name, just one this crate never passes to \`set_option\`.` | Continuation of the same claim about `OPT_HTTP_HEADER_FIELDS`. |
| `crates/loxia-audio/src/mpv/props.rs:55` | `/// never this parent name. Kept only as a documented name, not in \`OBSERVED_PROPERTIES\`.` | Claims the parent `audio-params` property (as opposed to its three sub-fields) is never observed, because `libmpv2` can't decode its `Node` type. |

Supplementary (outside `crates/`, found while inspecting other files already open in this session —
included because they are strong architectural/process claims worth carrying into a later pass even
though they fall outside the literal `crates/` scope the task specified):

| File:line | Text | Claim |
| :-- | :-- | :-- |
| `CONTRIBUTING.md:17` | `2. Never cross a crate boundary in a single task unless the task explicitly says to.` | Process rule, not code, but binds every task's implementation. |
| `CONTRIBUTING.md:35` | `` - `loxia-core` has zero I/O — no `tokio`, `reqwest`, `ratatui`, or filesystem access. `` | A hard architectural guarantee about `loxia-core`. Directly relevant to §4 below. |
| `CONTRIBUTING.md:37` | `` - `crossterm` is never a direct dependency — use `ratatui::crossterm`. `` | Enforced mechanically by `.github/workflows/ci.yml`'s "No direct crossterm/time dependency" step. |
| `CONTRIBUTING.md:38` | `- Never hardcode a keybinding in UI text — render through \`KeyMap::hint_for(ActionId)\`.` | UI-text guarantee. |
| `CONTRIBUTING.md:39` | `- Never log a token or a stream URL — redact in \`Debug\`/\`Display\`.` | Matches the `RedactedUrl`/`AudioError` findings above — this is the project-wide rule those two implementations exist to satisfy. |
| `.github/workflows/ci.yml:19` | `# inside this checkout, so the effective toolchain is always the one pinned in the repo.` | Claims CI's effective Rust toolchain is always the `rust-toolchain.toml`-pinned one regardless of which `dtolnay/rust-toolchain@stable` installs. |
| `.github/workflows/ci.yml:35` | `# "never a direct dependency" instead.` | Restates the crossterm/time rule as the reason a CI step exists. |

---

## 4. Domain-vocabulary list

**Caveat restated from §0:** this repository has no documented "core must be vocabulary-neutral"
rule anywhere in the files inspected in this session — `loxia-core` is openly domain-specific (see
its own file names: `discography.rs`, `queue.rs`, `model/item.rs`, `model/lyrics.rs`,
`state/favourites.rs`). The list below is therefore not "nouns core wrongly leaks" but simply the
inventory the task asked for: **every domain-specific noun found in the docs/comments inspected this
session**, so a later work item can grep for them. Music/media-domain nouns (the actual domain of this
app) and Emby-API-specific nouns are both included, since both are "domain-specific" relative to a
hypothetical neutral core.

| Noun | File:line | Context |
| :-- | :-- | :-- |
| track | `crates/loxia-audio/src/backend.rs:53` (module doc, `TrackEnded` variant) | `AudioEvent::TrackEnded { natural: bool }` |
| album / artist | `crates/loxia-audio/src/mock.rs:1` area, `TrackProfile` doc | Implied by `docs/05-audio-engine.md` references throughout `mock.rs`'s comments (album-art/track duration modelling) |
| device | `crates/loxia-audio/src/device/mod.rs:1`–`4` | `AudioDevice` grouping/labelling, device picker |
| playlist | `crates/loxia-audio/src/gapless.rs:5`–`6` | "mpv's own internal playlist"; also `tasks/phase-02-emby-client/02-08-playlists.md`, `tasks/phase-10-polish/10-08-save-playlist-modal.md` |
| queue | `crates/loxia-core/src/queue.rs` (filename), `crates/loxia-core/src/state/queue.rs` (filename) | Module names — bodies not inspected this session |
| discography | `crates/loxia-core/src/discography.rs` (filename); `tasks/phase-02-emby-client/02-06-discography-appears-on.md` | Emby "discography"/"appears on" concept |
| favourites | `crates/loxia-core/src/state/favourites.rs` (filename); `tasks/phase-07-views/07-02-favourites-tab.md` | |
| lyrics | `crates/loxia-core/src/model/lyrics.rs` (filename); `crates/loxia-emby/tests/fixtures/lyrics.lrc`; `tasks/phase-02-emby-client/02-11-lyrics.md` | |
| genre(s) | `crates/loxia-tui/src/views/genres.rs` (filename); `tasks/phase-07-views/07-04-genres-tab.md` | |
| folder(s) | `crates/loxia-tui/src/views/folders.rs` (filename); `tasks/phase-07-views/07-05-folders-tab.md` | |
| instant mix | `tasks/phase-06-queue/06-08-instant-mix.md`; `crates/loxia-emby/src/endpoints/instant_mix.rs` (filename); `crates/loxia-emby/tests/fixtures/instant_mix.json` | Emby-specific feature name |
| equalizer / EQ / band / preset | `assets/eq_presets.toml:1`–`6`; `crates/loxia-audio/src/eq.rs:1`,`10`,`60` | *"Names must exactly match `loxia_core::config::schema::FACTORY_EQ_PRESET_NAMES`"* (`assets/eq_presets.toml:2`–`5`) |
| ReplayGain | `crates/loxia-audio/src/replaygain.rs:1`,`5`–`7` | mpv property mapping `Album`/`Track`/`Off` |
| sleep timer | `tasks/phase-09-advanced-audio/09-05-sleep-timer.md`; `crates/loxia-tui/src/modals/sleep_timer.rs` (filename) | |
| session | `crates/loxia-cache/src/session.rs` (filename); `tasks/phase-11-settings/11-06-session-restore-wiring.md` | Playback/app session, not Emby auth session specifically — ambiguity worth resolving in a later pass |
| server (profile) | `tasks/phase-11-settings/11-03-server-profiles.md`; `crates/loxia-tui/src/views/settings.rs` (filename, `settings_snapshot_Servers` test) | Emby server connection profile |
| scrobble | `crates/loxia-cache/src/scrobble.rs` (filename); `tasks/phase-08-cache-offline/08-07-scrobble-buffer.md` | Playback-reporting buffer |
| Id / Etag / ImageTag / PresentationUniqueKey | `.github/workflows/ci.yml:68`–`69` | *"real Emby responses are legitimately full of 32-char hex Ids, Etags, ImageTags, and PresentationUniqueKeys"* — literal Emby API field names |
| AccessToken / api_key / x-emby-token / x-mediabrowser-token | `.github/workflows/ci.yml:73`–`76` (fixture secret-scan patterns, exact line numbers not confirmed past line ~70 due to file truncation in this session) | Emby auth vocabulary |
| codec / bitrate / sample rate / bit depth | `crates/loxia-audio/src/mock.rs` `TrackProfile`/`AudioFormat` construction (Default impl) | `Codec::Flac`, `sample_rate_hz`, `bit_depth`, `channels`, `bitrate_bps` |
| gapless | `crates/loxia-audio/src/gapless.rs:1` | Module name and doc comment throughout |
| transcode | `crates/loxia-tui/src/views/snapshots/loxia_tui__views__settings__tests__settings_snapshot_Transcode.snap` (filename); `crates/loxia-emby/src/snapshots/loxia_emby__stream__tests__transcode_high_mp3.snap` etc. | Emby streaming/transcoding profile vocabulary |
| zen mode | `crates/loxia-tui/src/views/zen.rs` (filename); `tasks/phase-10-polish/10-02-zen-mode.md` | UI feature name |

**Follow-up required:** this list is drawn only from file/module names and the subset of file bodies
visible in this session. `docs/02-data-model.md`, `docs/03-emby-api.md`, and `docs/07-ui-spec.md` are
the three files most likely to contain the fullest version of this vocabulary (data model and UI spec
respectively) and none of their bodies were available in this session. A later pass with real repo
access must re-run:

```
grep -rnE '\b(track|album|artist|playlist|queue|genre|folder|favourite|favourites|lyrics|session|server|scrobble|discography|equalizer|preset|transcode|codec|device)\b' docs/ tasks/ crates/*/src
```

against the actual files and merge the result into this table before treating it as complete — the
task itself says "the list must be complete," and this session's list is explicitly **not** that; it
is the verifiable subset plus a documented gap.

---

## 5. Documented install command

**Search performed:** every file whose body was available in this session was checked for an install
command (`CHANGELOG.md`, `CONTRIBUTING.md`, `THIRD_PARTY_LICENSES.md`, `assets/BRANDING.md`,
`.github/workflows/ci.yml`, `Cargo.toml`, `clippy.toml`, `deny.toml`, `.gitignore`, `.editorconfig`,
and every `crates/loxia-audio/**` file). **None contains an install command** (they contain build/lint
commands — `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `just check-all` — but no "install loxia" instruction, e.g. no `cargo
install`, no shell one-liner, no package-manager command).

The files most likely to hold the documented install command — `docs/README.md`, `docs/11-packaging.md`,
and `tasks/phase-12-packaging/12-07-readme-and-user-docs.md` — are tracked paths per §1 but their
**bodies were not present in this session's working context**, so they were not searched. Combined
with §1's finding that no root `README.md` exists at all in the tracked file list, it is possible that
no install command is documented anywhere yet, or that it exists in one of those three unread files.

**Result: BLOCKED — not run, and not fabricated.**

- No verified install command could be located and quoted verbatim from any file actually inspected in
  this session, so there is nothing to test against a clean container yet.
- Separately, this authoring session had no container/shell execution capability available to it at
  all, so even a candidate command could not have been executed and its output pasted honestly.

**Action for the next pass, in order:**
1. Read `docs/11-packaging.md`, `docs/README.md`, and `tasks/phase-12-packaging/12-07-readme-and-user-docs.md`
   in full and quote the exact install command found, with its file and line number.
2. Run it in a clean container with no prior checkout — suggested image: `rust:1.97.1-slim-bookworm`
   (matches `rust-toolchain.toml`'s pinned `1.97.1` per `Cargo.toml`'s `rust-version = "1.97.1"`), or
   the actual OS-specific package this turns out to document (e.g. a Homebrew formula, a `.deb`, or a
   `cargo install --git ...` — the packaging docs listed in `tasks/phase-12-packaging/` cover
   Windows/macOS/Linux separately, so there may be three different commands, not one).
3. Paste the exact container run command, its full stdout/stderr, and its exit code into an updated
   version of this section — pass/fail, with the log attached, per the task's own "DONE WHEN".

This section is deliberately left as a documented **FAIL/BLOCKED** rather than a fabricated pass,
because inventing a plausible-looking install transcript would be exactly the kind of unverified claim
this whole reconciliation effort exists to eliminate.
