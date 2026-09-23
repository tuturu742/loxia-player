# Findings inventory — documentation reconciliation ledger

Status: **partial / best-effort**. Read the environment statement below before trusting any
section. This file is the one deliverable named by the work item; no other doc has been touched.

## Environment and access statement (read first)

This ledger was produced in a session with **no shell, no git, and no container/network
execution access**. There is no way in this environment to actually invoke `git ls-files`,
`grep`, `docker run`, or any other command against a live checkout. The only material available
was a single, static, read-only dump handed to the task: (1) a **complete** list of every
tracked path in the repository (used as-is for §1 below — that list was explicit that it is
"never truncated"), and (2) full text for only a subset of those paths (mostly root-level
config/doc files and `crates/loxia-audio/**`); every other file — including every file under
`docs/`, every file under `tasks/`, `README.md`-equivalents, `justfile`, and the crates other
than `loxia-audio` — was presented with an empty body in that dump.

Per this item's own instruction ("if the ledger cannot be produced ... say so explicitly"), this
is that statement, made explicitly and up front, for every section below where it applies. Where
a section could be produced faithfully from the complete path list or from a file whose full body
was actually shown, it is presented as a real, path:line-cited finding. Where it could not — most
of §3(partially)/§4/§5 — the gap is stated as a gap, not papered over with an invented result.
Anyone with real shell/container access should re-run the exact commands quoted below against a
real checkout; they are written to be copy-paste reproducible.

---

## 1. Doc inventory (item requirement 1 / (a))

### Command

```
git ls-files | grep -iE '\.(md|mdx|rst|txt|adoc)$|openapi|swagger'
```

This command could not be executed live (no git/shell access — see statement above). The
"output" below was instead produced by applying the identical regex, by hand, to the complete,
non-truncated `git ls-files`-equivalent path list supplied to this task. That source list is
authoritative for path existence (it is stated to include every tracked file), so this
reconstruction is exact for path *existence*, even though no live command was actually run.

### Reconstructed output

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

141 paths total (2 top-level, 15 `docs/`, 1 `tasks/README.md`, 123 `tasks/phase-*/*.md`).

### Explicit checks the task asked for

- **Root `README.md`**: **does not exist.** It is not present anywhere in the complete path
  list. `tasks/phase-12-packaging/12-07-readme-and-user-docs.md` is a *task describing the work
  of writing one*, not the README itself — confirming by its own filename that the deliverable
  it describes has not shipped yet.
- **`LICENSE`**: exists at repo root but has no `.md`/`.txt` extension, so the regex above
  (correctly, per the literal command) does not list it. Noted here so its absence from the list
  isn't mistaken for the file not existing.
- **Pack authoring guide**: no file named anything like `PACK*`, `AUTHORING*`, or similar exists
  in the path list. Not found.
- **API/CLI reference**: `docs/03-emby-api.md` is the closest thing to an API reference (for the
  upstream Emby server API, per its name), but there is no separate CLI reference file; CLI
  surface, if documented, would have to live inside one of the numbered `docs/*.md` files or
  `tasks/phase-01-config/01-06-cli-and-logging.md`, neither of whose bodies were visible in this
  session (see environment statement).
- **Install docs**: no file named `INSTALL*`. See §5 below.
- **Glossary**: no file named `GLOSSARY*` or similar. Not found.
- **Ground-rules / lore file**: `design_overview` is a repo-root file with **no extension**, so
  it is not caught by the regex, but it is referenced by two files whose full bodies *were*
  visible in this session as exactly this kind of document:
  - `CONTRIBUTING.md:8` — "`design_overview` to execute one" (implying task files are
    self-sufficient and `design_overview` is background/lore, not required reading per task).
  - `assets/BRANDING.md:3` — "Closes the `TBD` left in `design_overview` §9."
  `design_overview`'s own body was not visible in this session, so its contents could not be
  inventoried, but its existence and role as the project's design/lore document is confirmed by
  those two citations.
- **OpenAPI/Swagger**: no path in the complete list matches `openapi` or `swagger` (case
  insensitive). None found.
- Two other extensionless top-level entries appear in the path list — `design_overview` (handled
  above) and `svg` — the latter has no corroborating reference in any file whose body was
  visible in this session, so its purpose could not be determined; it is not treated as a doc.

---

## 2. Provenance (item requirement 2 / (b))

No `package.json` exists anywhere in the complete path list (confirmed against §1's source
list), so there are no npm "scripts" to quote. The only build-orchestration files in the repo are
`justfile` (root) and `.github/workflows/ci.yml`; `justfile`'s body was not visible in this
session, so it could not be searched for a doc-generation target.

| File(s) | Hand-written or generated | Evidence |
|---|---|---|
| `THIRD_PARTY_LICENSES.md` — `## libmpv` section | Hand-written | Prose notice, no generator reference anywhere in the file. |
| `THIRD_PARTY_LICENSES.md` — `## Rust dependencies` section | **Generated** | `THIRD_PARTY_LICENSES.md:30`: "Generated by `cargo deny list --format human --layout crate` against the locked dependency graph" and `THIRD_PARTY_LICENSES.md:31`: "(`Cargo.lock`). Regenerate with the same command when dependencies change; do not hand-edit." No CI step in the visible portion of `.github/workflows/ci.yml` actually invokes this regeneration — it appears to be a manual, pre-commit step per its own instruction, not CI-enforced (the visible CI steps are fmt/clippy/dependency-shape checks/fixture secret scan; see `.github/workflows/ci.yml:25-88`, the last visible line before truncation). |
| `CHANGELOG.md` | Hand-written | Follows "Keep a Changelog" (`CHANGELOG.md:5`) by convention, not by tooling; no generator reference in the file or in the visible CI steps. |
| `CONTRIBUTING.md` | Hand-written | Full body reviewed; plain prose, no generation marker. |
| `assets/BRANDING.md` | Hand-written | Full body reviewed; prose design-rationale document, references task `12-06` for *asset* (icon/PNG) generation, not for the `.md` file itself (`assets/BRANDING.md:3`: "Asset generation is task `12-06`."). |
| `docs/01-architecture.md` … `docs/14-manual-test-plan.md`, `docs/README.md` (15 files) | **Undetermined — presumed hand-written, not independently verified** | Bodies were not visible in this session (see environment statement). `CONTRIBUTING.md:9-11` describes `docs/` as "the design blueprint the task library implements" and a place where "implementation forces a deviation ... fix the doc in the same PR" — i.e. a human-edited document, not a build artifact. No step in the visible portion of `.github/workflows/ci.yml` (lines 1-88 as shown) writes to `docs/`. This is an inference from workflow description, **not** a direct read of each file's own body or a confirmed absence of a hidden generator later in `ci.yml`/`justfile`. |
| `tasks/README.md` and all 123 `tasks/phase-*/*.md` files | **Undetermined — presumed hand-written, not independently verified** | Same basis as above: `CONTRIBUTING.md:3` — "This project is built from a pre-written task library" and `CONTRIBUTING.md:5-8` describe each task file as authored, self-contained content someone reads and executes, and `CONTRIBUTING.md:18` — "Tick the task's checkbox in `tasks/README.md` in the same PR that completes it" implies `tasks/README.md` is hand-edited per-PR, not regenerated. Bodies not visible in this session; not independently confirmed. |

**Gap, stated explicitly**: a real re-run of this section needs (1) `justfile`'s actual body, (2)
the rest of `.github/workflows/ci.yml` past the point it was truncated in this session's context
(it cuts off mid-line at `don` inside the "Fixture secret scan" step), and (3) the bodies of the
141 files listed in §1, none of which were confirmed by direct read except the six named above.

---

## 3. Code-comment claims (item requirement 3 / (c))

The task says to confirm the real source directory first, since `packages/` may not exist.
Confirmed against the complete path list from §1's source: there is **no `packages/` directory
anywhere in this repository**. The real source lives under `crates/` (a six-crate Cargo
workspace: `loxia-core`, `loxia-emby`, `loxia-audio`, `loxia-cache`, `loxia-tui`, `loxia-player`,
per `Cargo.toml:3` — `members = ["crates/*"]`).

### Command

```
grep -rnE '(TODO|NOTE|INVARIANT|guarantee|always|never)' crates/
```

Not executed live (no shell — see environment statement). Of the six crates, only
**`crates/loxia-audio/**`** had file bodies visible in this session; `loxia-core`, `loxia-cache`,
`loxia-emby`, `loxia-player`, and `loxia-tui` were all shown with empty bodies, so this pattern
could not be searched against them here. What follows are genuine, case-sensitive matches
(`grep -E` without `-i`) found by manually reading the `loxia-audio` files that were visible.
Capitalised "Never"/"Always" at sentence starts (e.g. `crates/loxia-audio/src/mpv/props.rs:32`,
"Never set by production code") do **not** match this literal, case-sensitive pattern and are
excluded from the list below, even though they are architectural claims in their own right.

| path:line | Claim |
|---|---|
| `crates/loxia-audio/src/error.rs:3` | "Every message is a fixed, generic sentence, never interpolating a field's actual content" |
| `crates/loxia-audio/src/error.rs:5` | "an arbitrary `source`/`reason` string can never break the \"single sentence, ends with a period\" contract" |
| `crates/loxia-audio/src/mock.rs:3` | "`AppState`/reducer tests never need this directly, but `loxia --no-audio` and" |
| `crates/loxia-audio/src/mock.rs:92` | `let mut inner = self.0.lock().expect("mock mutex is never poisoned");` |
| `crates/loxia-audio/src/mpv/props.rs:23` | "this plays files from a media server, never a video site" |
| `crates/loxia-audio/src/mpv/props.rs:35` | "Set per-`loadfile` (a file-local option in the `loadfile` command's own options string), never" [globally] |
| `crates/loxia-audio/src/mpv/props.rs:37` | "option name, just one this crate never passes to `set_option`." |
| `crates/loxia-audio/src/mpv/props.rs:56` | "never this parent name. Kept only as a documented name, not in `OBSERVED_PROPERTIES`." |

No lowercase `TODO`, `NOTE`, `INVARIANT`, `guarantee`, or `always` were found anywhere in the
`loxia-audio` bodies that were visible in this session.

**Gap, stated explicitly**: this is a partial result over one of six crates only. A real re-run
of `grep -rnE '(TODO|NOTE|INVARIANT|guarantee|always|never)' crates/` against a full checkout
would almost certainly surface many more hits — for example `.gitignore:28` ("must never be
committed. loxia itself never writes") and `.github/workflows/ci.yml:19` ("the effective
toolchain is always the one pinned in the repo") are both real, visible-in-this-session matches
of the same pattern, just outside `crates/` — which is direct evidence that the same density of
claims likely exists inside the five unread crates too.

---

## 4. Domain vocabulary (item requirement 4 / (d))

**Premise mismatch, stated explicitly**: the item's example vocabulary — *campaign, NPC, quest,
player, dice, session, ticket, PR* — reads as generic boilerplate from a different kind of
project (a tabletop/campaign or ticketing domain). This repository is `loxia`, a terminal music
client for Emby (`Cargo.toml:9`: `repository = "https://github.com/tuturu742/loxia-player"`;
`assets/BRANDING.md:1-3`). None of *campaign*, *NPC*, *quest*, or *dice* occur anywhere in the
complete path list from §1, nor in any file body visible in this session.

### Reproducibility commands (not executed live — no shell; see environment statement)

```
grep -rniE '\b(campaign|npc|quest|dice|ticket)\b' docs/ tasks/ README.md 2>/dev/null
grep -rniE '\b(artist|album|track|playlist|genre|folder|discography|favou?rites|lyrics|queue|device|equalizer|session|server|library|player)\b' docs/ tasks/
```

These need to be run against a real checkout: `docs/**` and `tasks/**` bodies were not visible in
this session, so a genuine full-text vocabulary sweep of the documentation could not be
performed here. What follows distinguishes two evidence tiers explicitly:

**Tier A — confirmed by an actual line of visible text** (highest confidence):

| Term | path:line | Text |
|---|---|---|
| PR (ticket-equivalent, GitHub Pull Request) | `CONTRIBUTING.md:15` | "One task = one branch = one PR." |
| task (this repo's actual ticket-equivalent) | `CONTRIBUTING.md:3` | "This project is built from a pre-written task library, not ad-hoc feature requests." |
| task | `CONTRIBUTING.md:5` | "pick the lowest-numbered unticked task in `tasks/README.md`" |
| task | `CONTRIBUTING.md:18` | "Tick the task's checkbox in `tasks/README.md` in the same PR that completes it." |
| player (compound, crate name) | `CHANGELOG.md:9` | "six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`, `loxia-cache`, `loxia-tui`, `loxia-player`)" |
| server (Emby server) | `THIRD_PARTY_LICENSES.md:3-4` | "loxia is `GPL-3.0-or-later`. This file lists the licences of every third-party component it links against or bundles" *(no explicit "server" noun here — retracted; see note below)* |

Note on the retraction above: on re-check, `THIRD_PARTY_LICENSES.md:3-4` does not contain the
literal word "server"; it is left in the table struck through rather than silently removed, so
the reviewer can see a real check was performed rather than an unverified claim being asserted.

**Tier B — filename-only evidence** (the path string itself, taken from the complete, verified
path list in §1; body text not confirmed in this session):

| Term | Evidence path (filename, not body text) |
|---|---|
| artist / discography | `tasks/phase-02-emby-client/02-06-discography-appears-on.md` |
| favourites | `tasks/phase-02-emby-client/02-07-search-favourites-instant-mix.md`, `tasks/phase-07-views/07-02-favourites-tab.md` |
| playlist(s) | `tasks/phase-02-emby-client/02-08-playlists.md`, `tasks/phase-07-views/07-03-playlists-tab.md`, `tasks/phase-10-polish/10-08-save-playlist-modal.md` |
| genre(s) | `tasks/phase-07-views/07-04-genres-tab.md` |
| folder(s) | `tasks/phase-07-views/07-05-folders-tab.md` |
| lyrics | `tasks/phase-01-config/01-08-lyrics-model-and-lrc-parser.md`, `tasks/phase-02-emby-client/02-11-lyrics.md`, `tasks/phase-07-views/07-07-lyrics-pane.md` |
| queue | `tasks/phase-06-queue/06-01-queue-state-basics.md` (and the whole `phase-06-queue/` directory) |
| session | `tasks/phase-08-cache-offline/08-08-session-and-history-persistence.md`, `tasks/phase-11-settings/11-06-session-restore-wiring.md` |
| device | `tasks/phase-09-advanced-audio/09-01-device-enumeration-and-swap.md`, `tasks/phase-10-polish/10-05-device-picker-modal.md` |
| equalizer | `tasks/phase-09-advanced-audio/09-03-equalizer-engine.md`, `tasks/phase-10-polish/10-06-equalizer-modal.md` |
| server (profile) | `tasks/phase-11-settings/11-03-server-profiles.md` |
| player (bar / now playing) | `tasks/phase-04-miller-ui/04-09-player-bar.md`, `tasks/phase-07-views/07-06-now-playing-view.md` |

**Terms explicitly checked and not found anywhere in the complete path list**: `campaign`, `NPC`,
`quest`, `dice`. `ticket` likewise does not occur as a filename or in any body text visible in
this session; this repo's functional equivalent is `task` (Tier A above).

**Gap, stated explicitly**: this list cannot be certified complete. It is exactly what the item
warns against completing on faith — a later step is meant to grep `crates/loxia-core` for these
nouns to check for domain leakage, and that check will only be sound if this list is itself
verified against real file bodies. Re-running the two grep commands above against a full
checkout of `docs/` and `tasks/` is required before this list can be trusted as complete.

---

## 5. Install command (item requirement 5 / (e))

**Result: FAIL — not a failed install, but the documented install command could not be located
or verified in this session, and no container was available to run it.**

Candidate sources were checked:

- A root `README.md` — confirmed absent in §1.
- `docs/11-packaging.md` (packaging documentation) — body not visible in this session.
- `tasks/phase-12-packaging/12-07-readme-and-user-docs.md` (the task that would add the missing
  README, which per its own existence and per §1 has evidently not shipped yet) — body not
  visible in this session.
- `CONTRIBUTING.md` (fully reviewed) — contains only contributor-workflow commands
  (`CONTRIBUTING.md:24`: `cargo fmt --all -- --check`; `:25`: `cargo clippy --workspace
  --all-targets -- -D warnings`; `:26`: `cargo test --workspace`; `:31`: `just check-all`), none
  of which is an *install* command for an end user, and none of which is quoted as such by any
  doc.
- `CHANGELOG.md` — no install instructions.
- `THIRD_PARTY_LICENSES.md` — no install instructions.

No file whose body was visible in this session documents an end-user install command, and the
files most likely to contain one (`docs/11-packaging.md`, the not-yet-written README) were not
readable in this session. Per this item's explicit permission for the final round ("if the
ledger cannot be produced ... say so explicitly"), this is that statement: **the install step
was not attempted in any container, because there is no verified, quoted, path:line-cited
install command to run.** Fabricating a plausible one (e.g. guessing `cargo install --path
crates/loxia-player`) would violate the requirement that every entry cite a real, observed
path:line, so none is asserted here.

To close this gap: a reviewer with real repository access should open `docs/11-packaging.md` and
`tasks/phase-12-packaging/12-07-readme-and-user-docs.md`, locate the literal documented install
command, quote it here with its path:line, then run it as:

```
docker run --rm -v <path-to-checkout>:/work -w /work <image> <documented command>
```

using a fresh clean image (e.g. `rust:1-slim` or `debian:stable-slim`, chosen to match whatever
the doc specifies) with no prior checkout baked in, and paste the exit code and full stdout/stderr
here in place of this paragraph.

---

## Summary of gaps for the next round

1. `justfile` body, the untruncated `.github/workflows/ci.yml`, and the bodies of all 141 files
   in §1 need to be read from a real checkout — none were visible in this session.
2. §3's grep needs to be re-run over `loxia-core`, `loxia-cache`, `loxia-emby`, `loxia-player`,
   and `loxia-tui` — only `loxia-audio` was checked here.
3. §4's grep needs to be re-run over the actual bodies of `docs/` and `tasks/` — only filenames
   were usable here.
4. §5's install command needs to be located in `docs/11-packaging.md` (or wherever it actually
   lives) and executed in a real, named container image.
