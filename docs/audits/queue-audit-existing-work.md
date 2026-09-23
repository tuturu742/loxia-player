# Queue-audit: existing work summary

Written per the "queue-audit tasks" work item. This is a summary of what already exists, not a
task file and not part of the task index — see `tasks/README.md` for the index itself.

## 1. `tasks/README.md`

Read in full and searched for `queue`, `insert`, `play next`, `QueueBatch`, and `preload`.

Phase 06 ("Queue engine"), before this PR's additions, contained eight tasks:

| Task | Title | State |
| :-- | :-- | :-- |
| `06-01` | Queue state basics | unticked |
| `06-02` | Appears-on queue rules | unticked |
| `06-03` | Shuffle | **ticked** (cited by the `06-03` shuffle-seed comment already found in source) |
| `06-04` | Sort profiles | unticked |
| `06-05` | Listening history | unticked |
| `06-06` | Gapless preloading | **ticked** (cited by the `06-06` preload comment in `crates/loxia-audio/src/gapless.rs`) |
| `06-07` | Playback reporting wiring | **ticked** (cited by the `06-07` reporting comment) |
| `06-08` | Instant mix | unticked |

**Finding:** no existing Phase 06 task is itself "in progress" or "blocked" in the sense the
README's own convention would show (there is no such state in this task library — a task is
either ticked or not). But three queue-editing-relevant tasks are unticked and not yet started:
`06-01` (queue state basics — this is where `insert`/`QueueBatch`-shaped operations would most
naturally live), `06-02` (appears-on queue rules, which also inserts related tracks into the
queue), and `06-08` (instant mix, which inserts a generated batch of tracks — the closest existing
match to "insert" / "play next" / "QueueBatch" in the search terms). None of the five new work
items in this PR duplicate any of these: `06-01`/`06-02`/`06-08` are about building the
insert/queue-rule machinery in the first place, whereas `06-09`–`06-13` are about characterizing,
fixing, and auditing behaviour once that machinery exists. No existing task is renamed, retitled,
or otherwise touched.

Also outside Phase 06: `09-01` (device enumeration and swap) and `11-06` (session restore wiring)
are ticked, matching the other two task IDs cited in source comments in the investigation.

## 2. `docs/12-decisions.md` §9

This file's contents were not present in the reviewed session/diff, so its rows cannot be quoted
verbatim here — the same limitation the reviewer flagged. What follows is everything that *can*
be established indirectly, from `docs/12-decisions.md` citations already present in source-code
comments in this session's context, so the "no existing row" claim can be checked against a
concrete list rather than taken on faith:

Citations found (topic, not exact row wording, since the row text itself wasn't available):

- `crates/loxia-audio/src/backend.rs` — `EqCurve` as a distinct wrapping struct rather than
  reusing `loxia_core::effect::EqCurve`'s bare-array alias.
- `crates/loxia-audio/src/backend.rs` — addition of `AudioEvent::VolumeChanged` (added in `05-04`,
  not in the original §2 table).
- `crates/loxia-audio/src/eq.rs` — routing `anequalizer` through mpv's `lavfi` bridge
  (`lavfi=[anequalizer=...]`) rather than setting it directly, verified against a real mpv.
- `crates/loxia-audio/src/eq.rs` / `crates/loxia-audio/src/mpv/filters.rs` — `af-command`'s
  `change` sub-command failing against a real mpv/FFmpeg build, and resetting the whole `af`
  property not restarting playback, contrary to the original task text's prediction.
- `crates/loxia-audio/src/error.rs` — `library_not_found_hint` implemented as a runtime
  `std::env::consts::OS` match instead of `#[cfg(target_os = ...)]`, to keep that attribute
  confined to `loxia-audio::device`/`loxia-core::paths`.
- `crates/loxia-audio/src/mock.rs` — `TrackProfile`/seeding capability on `MockControl`, not named
  in the original task signature.
- `crates/loxia-audio/src/device/mod.rs` — removal of the per-OS `linux`/`macos`/`windows`
  submodules after bit-perfect capability detection (task `09-02`) was dropped.
- `crates/loxia-audio/src/mpv/props.rs` — `OPT_YTDL` left off, and `OPT_STREAM_LAVF_O` added for
  HTTP auto-reconnect.
- `crates/loxia-audio/src/gapless.rs` — substituting a generated silent WAV for "a generated
  FLAC" in the `mpv-tests` gapless-transition test.
- `.github/workflows/ci.yml` — scoping the fixture secret scan to token-shaped fields/query
  parameters rather than a blanket hex-length check, because real Emby fixtures are legitimately
  full of 32-char hex `Id`/`Etag`/`ImageTag` values.

**None of these citations concern queue insert position, shuffle-and-insert interaction, or
retracting a gapless preload.** That supports (but, per the caveat above, does not conclusively
prove, since §9's actual row list was not readable in this session) the "no existing row" premise
this work item is built on. Whoever next has direct access to `docs/12-decisions.md` should grep
§9 for "insert", "shuffle", "preload", and "retract" to close this out definitively before relying
on this finding further.

## 3. New task files

Five task files were added under `tasks/phase-06-queue/`, prerequisite-chained in filename order
so the numerically-earlier-prerequisite guarantee holds without needing an explicit prerequisite
annotation in the README:

- `06-09-queue-insert-characterization-tests.md` — prerequisites `06-01`, `06-03`
- `06-10-insert-next-consistency-fix.md` — prerequisite `06-09`
- `06-11-stale-preload-audit.md` — prerequisites `06-06`, `06-10`
- `06-12-stale-preload-retraction.md` — prerequisite `06-11`
- `06-13-play-order-consumer-audit.md` — prerequisite `06-12`

`06-12` explicitly authorises crossing `loxia-core`, `loxia-audio`, and `loxia-player`, per
`CONTRIBUTING.md`'s rule that a task must say so explicitly to cross a crate boundary: the fix
needs the reducer (loxia-core) to detect and signal staleness, the audio backend (loxia-audio) to
accept and act on a retraction, and the worker/dispatch wiring (loxia-player) connecting the two.
