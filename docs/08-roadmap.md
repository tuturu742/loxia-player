# 08 — Roadmap

Thirteen phases. Each states its entry condition and exit criteria. The executable task files live
in `../tasks/phase-NN-*/` and are numbered so that running them in filename order is always valid.

```
00 ──► 01 ──► 02 ──┬──► 03 ──┬──► 04 ──► 07 ──┬──► 10 ──► 11 ──► 12
                   │         │                │
                   └──► 05 ──┴──► 06 ──► 08 ──┘
                             └──► 09
```

---

## Phase 00 — Scaffolding *(Agent A)*
**Entry:** empty repo.
**Exit:** `cargo test --workspace` green on a clean checkout; CI green on Linux, macOS, and Windows;
`cargo deny check` green. Six crates exist with their module files stubbed.

## Phase 01 — Config, paths & bootstrap *(Agent A)*
**Entry:** 00.
**Exit:** running the binary with no config creates one with `0600` permissions, logs to file,
opens and cleanly restores the terminal, and exits on `Ctrl+Q`. Config round-trips identically
through parse → write → parse.

## Phase 02 — Emby client *(Agent B)*
**Entry:** 01.
**Exit:** `cargo run -p loxia-emby --example probe` authenticates against the live server and prints
a correct ALBUMS / APPEARS ON split plus a track's lyric-stream reference. All fixture tests green.
The API audit (`02-01`) is signed off and its findings are recorded in `12-decisions.md` §7.

## Phase 03 — State machine core *(Agent A; parallel with 05)*
**Entry:** 01 (uses fixtures, not the network).
**Exit:** the navigation, modal, and keymap tests from `04-state-and-input.md` §10 are green,
including `default_keymap_has_no_conflicts`. The app runs and responds to keys with a debug state
dump. Idle CPU under 1 %.

## Phase 04 — Miller columns & first real UI *(Agent D)*
**Entry:** 02 + 03.
**Exit:** browse a real library Artists → Albums → Tracks with the correct section split, sliding
window, inline filter, and visual select. Snapshot tests pass at all three widths.

## Phase 05 — Audio MVP *(Agent C; parallel with 03 and 04)*
**Entry:** 01.
**Exit:** `cargo run -- --play-url <stream>` plays audio and reports position and format;
`MockEngine` passes its deterministic suite; the process exits cleanly within 5 s of quit.

## Phase 06 — Queue engine *(Agent A)*
**Entry:** 03 + 05.
**Exit:** every queue test in `04-state-and-input.md` §10 green. Playing an album advances
gaplessly. Shuffle/unshuffle round-trips with the current track preserved.

## Phase 07 — Remaining tabs *(Agent D)*
**Entry:** 04 + 06.
**Exit:** all nine sidebar tabs functional; every playlist mutation round-trips against the live
server; lyrics render for a track that has an `.lrc` sidecar.

## Phase 08 — Cache & offline *(Agent B)*
**Entry:** 06.
**Exit:** disconnecting the network mid-session enters Offline; downloaded content still browses and
plays; scrobbles buffer and drain on reconnect. The cache respects its size limit under a
fill-past-the-limit test.

## Phase 09 — Advanced audio *(Agent C)*
**Entry:** 05 + 06.
**Exit:** device hot-swap works on Linux and one of macOS/Windows; EQ changes are audible with no
playback restart; verified on Linux against
`/proc/asound/card*/pcm*p/sub*/hw_params`; the sleep timer fires with a clean fade-out.

## Phase 10 — Polish & integrations *(Agents D and C)*
**Entry:** 07 + 09.
**Exit:** every wireframe in `design_overview` §3 is reproducible in a real terminal (excluding the
cut features); media keys control playback on Linux via MPRIS and on one other OS.

## Phase 11 — Settings, keymapper & persistence *(Agent A)*
**Entry:** 10.
**Exit:** the app is fully configurable without hand-editing `config.toml`; a remapped key takes
effect immediately, persists, and conflicts are refused with the incumbent highlighted; session
restore resumes paused at the saved position.

## Phase 12 — Packaging & release *(Agent E; starts at 00, lands last)*
**Entry:** 11 for the release itself; CI work starts immediately.
**Exit:** tagging `v0.1.0` produces installable artifacts for all three platforms from CI, each
containing the GPL and LGPL licence texts and the mpv source pointer. A clean VM per OS installs and
runs them.

---

## Parallelization

- **00 → 01 → 02 is the critical path.** Do not fan out before phase 02 exists; everything
  downstream depends on real data shapes.
- After 02: Agent A (03, 06), Agent C (05), and Agent D (04, on fixtures) run concurrently.
- Agent E runs continuously from phase 00: CI, licence tracking, README.
- **Phase 10's album-art protocol detection** is the highest-risk remaining item; schedule it with
  slack. (Phase 09's bit-perfect work was the other, and has since been removed outright — see
  `docs/12-decisions.md`.)

## If scope must be cut for 0.1.0

Ship at phase 08 plus the EQ and device subset of 09, plus themes, help, and mouse from 10. Defer:
Sixel art, WebSocket remote control, the keymap editor UI (config-file remapping still works), and
multi-server switching UI. Record the cut in the release notes.

## Already cut from `design_overview`

Spectrum analyzer, crossfade, and 1–5 star ratings. See `12-decisions.md` §§2–3. These are not
deferred — they are removed from the product, and no task implements them.
