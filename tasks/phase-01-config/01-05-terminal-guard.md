# 01-05 · Terminal guard

**Phase:** 01 — Config · **Agent:** A · **Size:** S
**Prerequisites:** `00-02`
**Reference:** `docs/01-architecture.md` §4

## Goal
Enter and leave raw mode safely. The terminal must be restored on **every** exit path — clean quit,
panic, or signal — because a TUI that leaves the shell in raw mode on a crash is unusable.

## Files
- `crates/loxia-player/src/terminal.rs`
- `crates/loxia-player/src/main.rs`

## Specification

```
pub struct TerminalGuard { /* holds the Terminal<CrosstermBackend<Stdout>> */ }

impl TerminalGuard {
    pub fn enter(enable_mouse: bool) -> Result<Self>;
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>>;
    pub fn restore(&mut self) -> Result<()>;   // idempotent
}
impl Drop for TerminalGuard { /* calls restore(), ignoring errors */ }
```

`enter` performs, in order: `enable_raw_mode`, `EnterAlternateScreen`, `EnableMouseCapture` **only
when `enable_mouse`**, and hide cursor. `restore` reverses it exactly, and is safe to call twice.

Skipping mouse capture when disabled is deliberate: with capture on, the terminal's own text
selection stops working, so a user who turns the feature off must get their selection back.

**Panic hook**, installed in `main` *before* `enter`:
```
let default = std::panic::take_hook();
std::panic::set_hook(Box::new(move |info| {
    let _ = restore_terminal_raw();   // free function; does not need the guard
    default(info);
}));
```
Restoring first means the backtrace prints legibly instead of stair-stepping across the screen.

Use `ratatui::crossterm` throughout. A direct `crossterm` dependency is banned by `deny.toml`.

## Acceptance
- `terminal_guard_restore_is_idempotent` — call `restore()` twice; the second is `Ok`.
- `panic_hook_restores_before_default` — install the hook with a spy for the restore function,
  trigger a panic in a `catch_unwind`, assert the spy ran.
- Manual, recorded in the PR: run `cargo run -p loxia-player -- --panic-test` (a hidden debug flag that
  panics after startup) and confirm the shell is usable afterwards and the backtrace is readable.
- Manual: `--enable-mouse=false` leaves terminal text selection working.

## Done when
The global DoD in `tasks/README.md` is satisfied.
