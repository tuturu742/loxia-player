# 01-06 · CLI and logging

**Phase:** 01 — Config · **Agent:** A · **Size:** S
**Prerequisites:** `01-04`, `01-05`
**Reference:** `docs/01-architecture.md` §7

## Goal
Add argument parsing and file-based logging, then wire `main` end to end: parse args → load config →
init logging → enter the terminal → exit cleanly. This is the first runnable binary.

## Files
- `crates/loxia-player/src/main.rs`

## Specification

**CLI (`clap` derive):**

| Flag | Type | Meaning |
| :-- | :-- | :-- |
| `--config <PATH>` | `Option<PathBuf>` | override the config file location |
| `--server <ID>` | `Option<String>` | override `active_server` for this run |
| `--log-level <LEVEL>` | `Option<String>` | override `logging.level`; also `LOXIA_LOG` |
| `--no-audio` | `bool` | use `MockEngine` (wired in phase 05; accept it now) |
| `--doctor` | `bool` | run diagnostics and exit (implemented in `12-08`; here it prints "not yet implemented" and exits 0) |
| `--panic-test` | `bool`, `hide = true` | panic after startup, for the terminal-guard check |
| `--version`, `--help` | | clap built-ins |

**Logging:** `tracing-subscriber` with `EnvFilter`, sourced from `--log-level`, else `LOXIA_LOG`,
else `logging.level`. Sink is `tracing-appender` rolling **daily** into `paths.log_dir()`, retaining
`logging.max_files`. Non-blocking writer; keep the `WorkerGuard` alive for the process lifetime or
the last lines are lost on exit.

**stdout and stderr must never receive log output.** They belong to the TUI. A test asserts this.

Startup sequence in `main`:
1. Parse args.
2. Resolve `Paths`.
3. `load()` the config; collect warnings.
4. Init logging; log the version, config path, and every `ConfigWarning` at `warn`.
5. Install the panic hook, then `TerminalGuard::enter`.
6. Run the loop — for now, a placeholder that redraws a single line and waits for `Ctrl+Q` or
   `Ctrl+C`.
7. Restore the terminal, flush the log guard, exit 0.

Exit codes: `0` clean, `1` unrecoverable startup error (with a plain-text message on stderr **after**
the terminal is restored), `2` bad CLI usage (clap's default).

## Acceptance
- `logging_never_writes_to_stdout` — init logging with a temp dir, emit an `error!`, assert the
  captured stdout and stderr are empty and the log file contains the line.
- `log_level_precedence` — `--log-level` beats `LOXIA_LOG` beats `config.logging.level`.
- `config_warnings_are_logged` — a config with an unknown theme produces a `warn` line naming
  `ui.theme`.
- Manual, recorded in the PR: `cargo run -p loxia-player` with no config file creates one, opens the
  alternate screen, exits on `Ctrl+Q`, and leaves a usable shell. `--version` and `--help` work.

## Done when
The global DoD in `tasks/README.md` is satisfied.
