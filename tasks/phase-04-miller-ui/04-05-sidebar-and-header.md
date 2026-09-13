# 04-05 · Sidebar and header

**Phase:** 04 — Miller UI · **Agent:** D · **Size:** S
**Prerequisites:** `04-03`, `04-04`
**Reference:** `docs/07-ui-spec.md` §§3–4

## Goal
The two persistent chrome widgets: the nine-tab sidebar and the status header.

## Files
- `crates/loxia-tui/src/widgets/sidebar.rs`, `header.rs`

## Specification

**Sidebar** — nine rows, one per `Tab`:
```
▶ ♪ Now Playing              1
  ♥ Favourites               2
```
- The glyph column uses the theme's glyph set (ASCII fallback available).
- The trailing digit is the `Alt+N` hint, rendered in `Dim`. Derive it from
  `keymap.hint_for(ActionId::JumpTabN)` and show only the digit — **do not hardcode** the number,
  so a remapped tab jump displays correctly.
- The active tab uses `SelectionBg`/`SelectionFg`; `▶` marks it only when the sidebar itself has
  focus.
- Each row registers `HitTarget::SidebarTab`.
- Fixed width 16. Names are ellipsized if a translation ever exceeds it.

**Header** — a single line:
```
loxia │ <server name> │ <active tab> │ <badges> │ <clock>
```
Badges, right-aligned and present only when active, in this order:
`OFFLINE` (Error style) · `↓<n>` downloads (Accent) · `⇄` shuffle (Accent) · `🔁`/`🔂` repeat ·
`⏱<mm>m` sleep timer · `⚠<n>` config warnings (Warning).

The clock reads from `state`, not from `now()` — the runtime supplies the timestamp on each `Tick`.
A widget that reads the clock directly makes every snapshot test flaky.

Elision order as width shrinks: clock → server name → active tab. Badges are never dropped; they
are the ones that carry information the user cannot get elsewhere.

Neither widget mutates state.

## Acceptance
- `sidebar_snapshot_default`, `sidebar_snapshot_focused`
- `sidebar_marks_active_tab`
- `sidebar_hint_reflects_remapped_key` — remap `JumpTab3` and assert the rendered hint changes.
- `sidebar_registers_nine_hit_targets`
- `header_badges_appear_only_when_active` — table test over each badge.
- `header_elides_clock_first_then_server`
- `header_snapshot_offline_with_downloads`
- `header_uses_state_clock_not_system_clock` — two renders with the same state are byte-identical.

## Done when
The global DoD in `tasks/README.md` is satisfied.
