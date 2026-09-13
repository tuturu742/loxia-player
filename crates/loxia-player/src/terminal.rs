//! Raw mode, alt screen, mouse capture, panic-safe restore.
//!
//! Uses `ratatui::crossterm` exclusively — a direct `crossterm` dependency is banned
//! (`deny.toml`, `docs/13-dependencies.md` rule 1) because two copies of the crate in one binary
//! risk two incompatible `Event` types.

use std::io::{self, Stdout};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{
    DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Owns the terminal for the app's lifetime: raw mode, the alternate screen, optional mouse
/// capture, and a hidden cursor. `restore` (called explicitly or via `Drop`) reverses all of it
/// and is safe to call more than once — the second call is a no-op.
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restored: bool,
}

impl TerminalGuard {
    /// Enters raw mode, the alternate screen, and (when `enable_mouse`) mouse capture, then hides
    /// the cursor. Mouse capture is skipped entirely when disabled — not merely left unused —
    /// because enabling it turns off the terminal's own text selection, which a user who disabled
    /// the feature must get back.
    ///
    /// Focus-change reporting (`10-10`) is enabled unconditionally, unlike mouse capture — it has
    /// no equivalent user-visible trade-off to opt out of, and desktop notifications need it to
    /// know whether the terminal currently has focus. A terminal that doesn't support the
    /// underlying escape sequence simply never sends `FocusGained`/`FocusLost`, which is exactly
    /// `AppState::terminal_focused`'s already-handled "unknown" case.
    pub fn enter(enable_mouse: bool) -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        if enable_mouse {
            execute!(stdout, EnableMouseCapture)?;
        }
        execute!(stdout, EnableFocusChange)?;
        execute!(stdout, Hide)?;

        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok(TerminalGuard {
            terminal,
            restored: false,
        })
    }

    /// The render loop's (`crate::runtime::run_terminal`) handle onto the real terminal.
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Reverses everything `enter` did. Idempotent: a second call is a no-op `Ok(())`. Shared with
    /// the panic hook's free-function teardown (`restore_terminal_raw`) so there is exactly one
    /// place that knows the exit sequence.
    pub fn restore(&mut self) -> anyhow::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        restore_terminal_raw();
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// The same teardown `TerminalGuard::restore` performs, callable with no guard instance — this is
/// what the panic hook uses, since a panic hook is a global closure with no access to whichever
/// `TerminalGuard` the running code happens to hold. Disabling mouse capture and leaving the
/// alternate screen are harmless no-ops when those weren't active, so this can run unconditionally
/// regardless of whether mouse capture was ever enabled.
pub fn restore_terminal_raw() {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        DisableMouseCapture,
        DisableFocusChange,
        LeaveAlternateScreen,
        Show
    );
}

/// Installs a panic hook that restores the terminal *before* running the previously-installed
/// (default) hook, so a panic's backtrace prints legibly instead of stair-stepping across a raw,
/// alternate-screen terminal. Must be installed before `TerminalGuard::enter`.
pub fn install_panic_hook() {
    install_panic_hook_with(restore_terminal_raw);
}

fn install_panic_hook_with(restore_fn: impl Fn() + Send + Sync + 'static) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_fn();
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn panic_hook_restores_before_default() {
        static SPY_CALLED: AtomicBool = AtomicBool::new(false);
        SPY_CALLED.store(false, Ordering::SeqCst);

        // Save whatever hook is currently active (the test harness's own, typically) so it can be
        // put back afterwards — this test must not permanently alter the process's panic hook.
        let previous = std::panic::take_hook();
        install_panic_hook_with(|| SPY_CALLED.store(true, Ordering::SeqCst));

        let result = std::panic::catch_unwind(|| {
            panic!("terminal guard test panic");
        });

        // Restore the original hook before asserting, so a failed assertion here still leaves
        // the process's panic hook exactly as this test found it.
        std::panic::set_hook(previous);

        assert!(result.is_err());
        assert!(
            SPY_CALLED.load(Ordering::SeqCst),
            "panic hook's restore spy did not run"
        );
    }
}
