# Queue-audit findings

Written for the "register the queue-audit tasks" work item. Summarises what already existed in
the task library and the decision log before the five new task files below were added, so the
next person doesn't redo this investigation.

## 1. `tasks/README.md` — existing queue-editing coverage

Read in full. Phase 06 ("Queue engine") is `tasks/phase-06-queue/06-01` through `06-08`:

- `06-01` queue state basics
- `06-02` appears-on queue rules
- `06-03` non-destructive shuffle
- `06-04` sort profiles
- `06-05` listening history
- `06-06` gapless preloading
- `06-07` playback reporting wiring
- `06-08` instant mix

All eight are ticked `[x]` in `tasks/README.md`, consistent with `crates/loxia-player/src/main.rs`
citing `06-06`/`06-07` and `09-01`/`11-06` as already-implemented behaviour in inline comments.
`12-01` through `12-08` (the entire Phase 12 — Packaging section) are unticked — `main.rs` itself
prints "not yet implemented (see task 12-08)" for the doctor subcommand, and none of the other
seven packaging tasks (cargo-dist setup, per-OS packaging, licence compliance, branding assets,
README/user docs) show any corresponding implementation elsewhere in the tree either. All eight are
unrelated to the queue.

Searching the full task list for `queue`, `insert`, `play next`, `QueueBatch`, and `preload` turned
up no task — ticked or not — whose title or scope is "insert consistency", "stale preload
retraction", or an audit of queue-order consumers. `06-01` covers queue state *basics* (append,
remove, reorder) and `06-06` covers *establishing* a gapless preload, but neither task's title or
position in the library addresses what happens when a queue mutation (insert-next, shuffle re-roll,
remove-next) arrives while a preload for the old "next" track is already in flight. **Conclusion:
no existing task covers any of the five items below; none of them duplicate prior work.**

## 2. `docs/12-decisions.md` — existing rows

Read in full, including §9 (the deviation log) in its entirety. §9 contains no row addressing:

- queue insert position (where `InsertNext` places a track in `play_order`),
- the shuffle-and-insert interaction (whether that position is the same when shuffle is on or off),
- retracting a gapless preload once the queue's next track changes underneath it.

Other §9 rows referenced elsewhere in the repo are about unrelated subsystems: the `EqCurve`
wrapper struct kept distinct from `loxia_core::effect::EqCurve` (cited in
`crates/loxia-audio/src/backend.rs`), and the `VolumeChanged` audio event added to cover a gap in
the property→event mapping (cited in the same file). Neither is about queue ordering or preloading.
**Conclusion: none of the three rows described in the task brief exist yet.** The retraction task
(`06-12`) is expected to add the "retracting a gapless preload" row itself, once it lands a real
fix, per `CONTRIBUTING.md`'s normal rule ("if implementation forces a deviation from `docs/`, fix
the doc in the same PR").

## 3. New task files registered

Added to `tasks/phase-06-queue/`, in prerequisite order (each depends only on numerically earlier
tasks, so the "always take the lowest unticked task" rule in `tasks/README.md` still holds):

| ID | File | Prerequisites | Crate(s) |
| :-- | :-- | :-- | :-- |
| `06-09` | `06-09-queue-insert-characterization-tests.md` | `06-01`, `06-02`, `06-03` | `loxia-core` |
| `06-10` | `06-10-fix-insert-next-consistency.md` | `06-09` | `loxia-core` |
| `06-11` | `06-11-stale-preload-audit.md` | `06-06`, `06-10` | `loxia-core`, `loxia-audio`, `loxia-player` (read-only) |
| `06-12` | `06-12-stale-preload-retraction.md` | `06-11` | `loxia-core`, `loxia-audio`, `loxia-player` (explicitly authorised) |
| `06-13` | `06-13-play-order-consumer-audit.md` | `06-10`, `06-12` | `loxia-core`, `loxia-tui`, `loxia-player` (read-only) |

All five are unticked in `tasks/README.md`, inserted immediately after `06-08` and before the
Phase 07 section, per the existing "execute in filename order" convention.

`06-12` is the only one that touches production code in more than one crate in a single PR; its
task file carries an explicit crate-boundary authorisation per `CONTRIBUTING.md` rule 2, naming
`loxia-audio`, `loxia-core`, and `loxia-player` by name. `06-11` and `06-13` read across crate
boundaries but do not modify production code outside `loxia-core` — they are audits that record
findings as tests/docs within `loxia-core`, so they do not require the same authorisation.
