# Queue-audit registration — investigation notes

Not part of the task index (`tasks/README.md` is the executable list; this is background for the
PR that registered `06-09`..`06-13`).

## What already existed before this change

- **`tasks/README.md`**: read in full. The only queue-editing tasks already present were
  `06-01` (queue state basics) through `06-08` (instant mix), all ticked. None of them cover
  queue-insert characterization, the insert-next shuffle inconsistency, stale-preload retraction,
  or a `play_order` consumer audit — so none of the five new tasks duplicate existing work.
  - `06-03` is titled **"non-destructive shuffle"** (not "no-repeat shuffle" — an earlier draft of
    this change mis-cited it, and mis-titled several other already-ticked entries and phase
    headings; that draft has been reverted).
- **`docs/12-decisions.md`**: this repository snapshot does not include its contents, so its §9
  (deviation log) could not be inspected for an existing row on queue insert position, the
  shuffle/insert interaction, or gapless-preload retraction. This is the one open item from the
  original investigation: **not verified**. It is plausible no such row exists yet, since each of
  `06-10` and `06-12` is written to add its own §9 row when it lands — but that is an inference,
  not a confirmed fact from reading the file.

## What this change adds

Five new task files under `tasks/phase-06-queue/`, registered in `tasks/README.md` under the
existing `### Phase 06 — Queue engine` heading, in prerequisite order:

| Task | Depends on | Purpose |
| :-- | :-- | :-- |
| `06-09` queue-insert characterization tests | `06-01`, `06-03` | Pins down current insert-next behaviour, unshuffled and shuffled, as a baseline. No behaviour change. |
| `06-10` insert-next consistency fix | `06-09` | Fixes the shuffle/insert-next inconsistency `06-09` documents; adds the `docs/12-decisions.md` §9 row. |
| `06-11` stale-preload audit | `06-06`, `06-10` | Inventories every place a queue mutation can leave an issued `Preload` command stale, including consumers outside `loxia-core`. No behaviour change. |
| `06-12` stale-preload retraction | `06-11` | Fixes what `06-11` finds. Explicitly authorised to cross `loxia-core`, `loxia-audio`, and `loxia-player` in one PR, since the fix requires all three to change together. Adds the `docs/12-decisions.md` §9 row. |
| `06-13` play-order consumer audit | `06-01`, `06-10` | Inventories every direct reader of `play_order` so future changes don't repeat the same class of bug. No behaviour change. |

## Review fixes applied in this revision

1. `tasks/README.md` now only differs from the original in the task count (109 → 114) and the
   five new lines under the pre-existing `### Phase 06 — Queue engine` heading. Every other line,
   including phase heading levels (`###`, not `##`) and titles, and every already-ticked task's
   description, has been reverted to its original text.
2. `06-10`'s repeated-insert rule no longer contradicts itself: call order wins, the k-th insert
   (0-indexed) lands at `current_index + 1 + k`, and earlier inserts are never displaced. The
   rationale and the `insert_next_stacks_contiguously_in_call_order` acceptance test both match
   that rule now.
3. `06-13`'s prerequisites (`06-01`, `06-10`) now match between its task file and its
   `tasks/README.md` line.
