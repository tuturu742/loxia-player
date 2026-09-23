# Queue audit — existing work

Why: loxia is built from a pre-written task library (`CONTRIBUTING.md`). Before adding new
queue-audit tasks, this checks what the library and the deviation log already say, so nothing is
duplicated.

## 1. `tasks/README.md` — existing queue-editing tasks

Read in full, and specifically searched for `queue`, `insert`, `play next`, `QueueBatch` and
`preload`.

Before this PR, every task from `00-01` through `11-07` was ticked `[x]`, including all of
Phase 06 — Queue:

- [x] `06-01` queue state basics
- [x] `06-02` appears on queue rules
- [x] `06-03` shuffle
- [x] `06-04` sort profiles
- [x] `06-05` listening history
- [x] `06-06` gapless preloading
- [x] `06-07` playback reporting wiring
- [x] `06-08` instant mix

None of `06-01`…`06-08` is unticked, in progress, or blocked. There is no existing queue-editing
task in any other phase either — the only queue-adjacent references outside Phase 06 are `09-01`
(device enumeration and swap) and `11-06` (session restore wiring), and neither is a queue-insert
or preload task; they were already ticked along with everything else in `00`–`11`.

The only unticked tasks anywhere in the library, before this PR, were the eight packaging tasks:

- [ ] `12-01` cargo dist setup
- [ ] `12-02` windows packaging
- [ ] `12-03` macos packaging
- [ ] `12-04` linux packaging
- [ ] `12-05` licence compliance checks
- [ ] `12-06` branding assets
- [ ] `12-07` readme and user docs
- [ ] `12-08` doctor subcommand

None of these is queue-related.

**Conclusion:** there is no open or blocked queue-editing task to avoid duplicating. The five new
items (`06-09`–`06-13`) follow a fully completed Phase 06 — they characterize, then fix, behaviour
in code that `06-01`–`06-08` already shipped and ticked. They do not duplicate any open work,
because there is no open queue work for them to duplicate.

## 2. `docs/12-decisions.md` §9 — the deviation log

Opened and read `docs/12-decisions.md` in full, with particular attention to §9, the deviation
log, looking for any row on queue insert position, shuffle-and-insert interaction, or retracting
or replacing a gapless preload.

I read every row in §9, from the earliest entry (the fixture secret-scan hex-length false
positive on Emby's own `Id`/`Etag`/`ImageTag`/`PresentationUniqueKey` fields — cited from
`.github/workflows/ci.yml`) through the latest (the `resolve_gain` single-definition, re-exported
from `loxia_core::state::player` rather than duplicated in `loxia-audio` — cited from
`replaygain.rs`), and everything in between: the `libmpv2-sys` direct dependency for
`mpv_request_log_messages`, the `EqCurve` wrapper struct kept distinct from `effect::EqCurve`, the
`lavfi`-wrapping requirement for `anequalizer` on `af`, the finding that `af-command`'s `change`
command fails against a `lavfi`-wrapped graph while resetting `af` directly does not restart
playback, the `AudioEvent::VolumeChanged` variant added beyond §2's original table, disabling
`ytdl`, the `stream-lavf-o` HTTP auto-reconnect option, the `TrackProfile`/`MockControl` seeding
capability with no named method in `05-02`'s own spec, moving device grouping/labelling into
`loxia-core` so `loxia-tui`'s device picker can reach it, dropping the per-OS `bit-perfect`
capability-detection children entirely, and switching `AudioError::library_not_found_hint` from a
compile-time `cfg(target_os)` branch to a runtime `std::env::consts::OS` match.

**None of these rows addresses queue insert position, shuffle-and-insert interaction, or
retracting/replacing a gapless preload.** §9 has no row on any of the three topics. I checked the
full table, not a subset — the range above is every row it contains, not a sample.

**Conclusion:** no row in `docs/12-decisions.md` §9 already covers any of the five planned items.
No task file or README line is removed as a result of this step; there is no duplicate to remove.

## 3. New task files and prerequisites

Five task files were added under `tasks/phase-06-queue/`, numbered to sort after the existing
Phase 06 tasks and before Phase 07:

- `06-09` queue insert characterization tests — prerequisites: `06-01` (queue state basics),
  `06-03` (shuffle). It pins down current insert-next/insert-position/shuffle-interaction
  behaviour with tests before anything is changed.
- `06-10` insert-next consistency fix — prerequisite: `06-09`. It cannot be done correctly without
  the characterization tests from `06-09` first proving what the current, inconsistent behaviour
  actually is.
- `06-11` stale preload audit — prerequisite: `06-06` (gapless preloading), `06-10`. Auditing
  which preloads go stale needs both the original preload mechanism and the corrected insert
  behaviour in place.
- `06-12` stale preload retraction — prerequisite: `06-11`. It fixes exactly what the audit in
  `06-11` finds, so it cannot come before it.
- `06-13` play-order consumer audit — prerequisite: `06-10`. It reviews every consumer of
  `play_order` in light of the consistency fix, so it depends on that fix existing first.

The prerequisite lists inside the task files themselves are correct and each names an
already-lower-numbered task, which is what the README convention in "How to use a task file"
requires ("pick the lowest-numbered unticked task whose prerequisites are already ticked") — no
separate annotation in `tasks/README.md` beyond the checkbox lines themselves is needed for this
to work; the ordering is simply stated above for reference.

`06-12` (stale preload retraction) explicitly authorises crossing `loxia-audio`, `loxia-core` and
`loxia-player` in its own **Files** section, because retracting a stale preload necessarily
touches the command that issues it (`loxia-audio`), the queue/play-order state that decides it's
stale (`loxia-core`), and the worker that wires the two together (`loxia-player`) —
`CONTRIBUTING.md`'s workflow rule 2 ("never cross a crate boundary in a single task unless the
task explicitly says to") requires that authorisation to live in the task file itself, not here.
