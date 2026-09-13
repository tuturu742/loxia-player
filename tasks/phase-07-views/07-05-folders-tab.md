# 07-05 · Folders tab

**Phase:** 07 — Views · **Agent:** D · **Size:** S
**Prerequisites:** `07-04`
**Reference:** `docs/07-ui-spec.md` §9

## Goal
Raw directory-tree browsing, for libraries whose metadata is poor and whose folder structure is the
real organisation.

## Files
- `crates/loxia-core/src/reducer/nav.rs` (extend)
- `crates/loxia-player/src/workers/network.rs` (extend)

## Specification

A Miller stack of `Folders { of_parent }` columns, each fetched with
`items::folder_children(parent)` — **non-recursive**, so a directory with 50,000 files below it
still opens instantly.

**Row rendering:** folders show `📁` and sort first; audio files show `♪`. Within each group, sort by
name using the same natural-order comparison as `06-04` so `track2` precedes `track10`. Non-audio
files are omitted entirely.

**Depth.** The stack can grow arbitrarily deep; the sliding window shows the last three levels with
the parent path indicator. There is no depth cap, but a `debug` log line at depth 20 helps diagnose
a pathological library.

**Queueing:**
- `a` / `Enter` on a **folder** queues every audio file directly inside it, not recursively. The
  toast states the count so the non-recursive behaviour is discoverable.
- `A` on a folder queues **recursively**, emitting a recursive fetch first. This is the one place
  where `A` means "more" in a directional rather than compositional sense; it is the natural reading
  and matches the `a`/`A` pattern elsewhere.
- `a` on a file queues that file.

**Pagination** applies per directory at 200 entries, as elsewhere.

**Empty state:** `this folder has no audio files`.

**Offline:** folders are not represented in the download sidecars, so the tab shows
`folder browsing needs a connection` when offline rather than a misleading partial tree.

## Acceptance
- `folders_sort_before_files`
- `natural_order_within_groups`
- `non_audio_files_are_omitted`
- `fetch_is_not_recursive` — assert the emitted query lacks `Recursive=true`.
- `queue_folder_is_shallow_and_toasts_count`
- `queue_folder_recursive_with_shift`
- `deep_stack_slides_window`
- `pagination_at_200_per_directory`
- `offline_shows_explanatory_state`
- `folders_snapshot`, `empty_folder_snapshot`

## Done when
The global DoD in `tasks/README.md` is satisfied.
