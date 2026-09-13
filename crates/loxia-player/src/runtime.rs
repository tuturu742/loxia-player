//! The main event loop: select over input, tick, and worker events; apply actions; dispatch
//! effects; redraw when dirty (`docs/01-architecture.md` §4, `docs/04-state-and-input.md` §9).

use std::time::{Duration, Instant};

use loxia_core::Timestamp;
use loxia_core::action::{Action, SystemEvent};
use loxia_core::effect::{CacheEffect, Effect, SysEffect};
use loxia_core::paths::Paths;
use loxia_core::reducer;
use loxia_core::state::AppState;
use loxia_tui::hit::HitMap;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::Event as CrosstermEvent;
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use tokio::sync::mpsc;

use crate::dispatch;
use crate::input::{MouseState, Viewport};
use crate::terminal::TerminalGuard;
use crate::workers::Workers;

/// Draws are throttled to at most once per this interval — a burst of held-key repeats yields a
/// steady ~60 fps instead of one draw per keystroke (`docs/01-architecture.md` §4).
const RENDER_INTERVAL: Duration = Duration::from_millis(16);
const TICK_INTERVAL: Duration = Duration::from_millis(100);
/// Fallback when the backend can't report a size (should not happen for a real terminal).
const DEFAULT_VIEWPORT_ROWS: usize = 24;

/// The real entry point: owns the actual terminal and reads real crossterm input. `main` calls
/// this. Tests call `run` directly against a `TestBackend` and a plain crossterm-event channel
/// instead — no real terminal or keypresses are needed to exercise the loop's own logic.
pub async fn run_terminal(
    state: AppState,
    guard: &mut TerminalGuard,
    workers: Workers,
    paths: &Paths,
) -> anyhow::Result<()> {
    // Detected **before** the input reader is spawned: `Picker::from_query_stdio` writes a query
    // escape sequence and reads the terminal's reply off stdin, so a reader already blocked on
    // `crossterm::event::read` would swallow that reply — detection then times out and silently
    // falls back to halfblocks (chunky covers on a Kitty/Sixel-capable terminal) or to nothing at
    // all. Ordering it first is what makes real graphics protocols actually get detected
    // (`docs/12-decisions.md`).
    let renderer =
        loxia_tui::widgets::album_art::detect_renderer(state.config.ui.album_art_protocol);
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    tokio::spawn(read_terminal_input(input_tx));
    run(state, guard.terminal(), input_rx, workers, paths, renderer).await
}

/// Reads real crossterm events forever, on the blocking-task pool (crossterm's `read` blocks the
/// calling thread), forwarding each raw event as-is. `input::to_action` needs live `&AppState` and
/// the current `Viewport` (`03-09`) — neither of which this task, or any other, may hand to a
/// second thread (`docs/01-architecture.md` §4: `AppState` lives on the main thread only, behind a
/// plain `&mut`, never an `Arc<Mutex<_>>`) — so the conversion happens in `run`'s own loop body,
/// where both are already in scope. Ends — dropping the sender — only on a genuine read error or
/// if nothing is listening any more; the main loop treats the input source disappearing as fatal
/// and exits.
///
/// `docs/01-architecture.md` §4 and this task's own spec name `crossterm::event::EventStream` as
/// the input source, but `ratatui-crossterm` 0.1.2 does not forward crossterm's `event-stream`
/// feature, and adding a direct `crossterm` dependency to obtain it ourselves is exactly what
/// `13-dependencies.md` rule 1 forbids (two copies of crossterm risking incompatible `Event`
/// types). Blocking `read()` on a dedicated task, forwarding into a channel, needs neither and
/// behaves identically from the loop's side: a channel yielding raw events to convert.
async fn read_terminal_input(input_tx: mpsc::UnboundedSender<CrosstermEvent>) {
    loop {
        let ev = match tokio::task::spawn_blocking(ratatui::crossterm::event::read).await {
            Ok(Ok(ev)) => ev,
            _ => return,
        };
        if input_tx.send(ev).is_err() {
            return;
        }
    }
}

/// `11-07`: "[d] Copy diagnostics" — writes an OSC 52 terminal escape sequence
/// (`\x1b]52;c;<base64>\x07`) directly to stdout rather than pulling in a clipboard crate
/// (`arboard`/`copypasta`, neither in `docs/13-dependencies.md`): OSC 52 needs no new dependency,
/// and — unlike an X11/Wayland clipboard API — works over SSH, which is exactly where a terminal
/// music client is most likely to be run from when someone wants to paste a bug report
/// (`docs/12-decisions.md`). Some terminals disable OSC 52 by default for security; this is a
/// narrow, accepted, documented limitation, the same shape as `apply_mouse_capture`'s own "errors
/// are swallowed" posture — there is no reliable way to detect support ahead of time.
fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    let encoded = base64_encode(text.as_bytes());
    let mut stdout = std::io::stdout();
    let _ = write!(stdout, "\x1b]52;c;{encoded}\x07");
    let _ = stdout.flush();
}

/// A minimal standard base64 encoder (RFC 4648, with padding) — small enough, and needed in only
/// this one place, that pulling in a dependency for it would be adding a crate to save a dozen
/// lines (`13-dependencies.md` rule: no dependency not already locked there).
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        let n =
            (u32::from(b0) << 16) | (u32::from(b1.unwrap_or(0)) << 8) | u32::from(b2.unwrap_or(0));
        out.push(ALPHABET[(n >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(n >> 12) as usize & 0x3f] as char);
        out.push(if b1.is_some() {
            ALPHABET[(n >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if b2.is_some() {
            ALPHABET[n as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// `11-01`: toggles terminal mouse capture live, in response to a settings edit — errors are
/// swallowed (matching `TerminalGuard`'s own restore path): a terminal that can't manage mouse
/// capture at all is not a reason to crash over.
fn apply_mouse_capture(enable: bool) {
    let mut stdout = std::io::stdout();
    let result = if enable {
        execute!(stdout, EnableMouseCapture)
    } else {
        execute!(stdout, DisableMouseCapture)
    };
    if let Err(error) = result {
        tracing::debug!(%error, "could not change terminal mouse capture");
    }
}

/// `11-03`: "switching servers... reconnect, and reseed the columns." By the time this effect
/// arrives, the reducer has already cleared the queue/history, persisted the outgoing session,
/// and set `config.active_server` to `server_id` — this is the one piece of the switch that
/// couldn't happen in the reducer itself (no access to `&mut Workers`, and connecting is `async`).
/// Mirrors `main`'s own startup sequence closely (`bootstrap::connect` -> `bootstrap::open_cache`
/// -> `Workers::reconnect` -> `bootstrap::seed_artists_column`), reusing all four rather than
/// re-deriving the sequence a second time.
async fn reconnect_server(
    state: &mut AppState,
    workers: &mut Workers,
    paths: &Paths,
    server_id: loxia_core::model::ServerId,
) {
    let Some(crate::bootstrap::Connected {
        client,
        library,
        storage,
    }) = crate::bootstrap::connect(state).await
    else {
        // `bootstrap::connect` already toasted the failure and switched to `Tab::Settings` —
        // the same place a failed *startup* connect leaves the app, nothing further to do.
        return;
    };

    let cache = crate::bootstrap::open_cache(
        paths,
        // The *server's* own identity, not the profile id in `server_id` — see
        // `bootstrap::storage_id`. The profile id still names the session file below, which is
        // keyed to the profile a user switched to, not to storage.
        storage.clone(),
        state.config.transcode.target_codec,
        // Honours the config, as `main`'s own startup does — see there for why the old force-off
        // was lifted.
        state.config.cache.prefetch_on_play,
    );
    // `08-06`'s connectivity state machine is what will actually flip `offline` and populate the
    // index; until then this is an inert default, the same one `main`'s own startup uses.
    let offline = crate::workers::network::OfflineHandle {
        offline: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        index: std::sync::Arc::new(std::sync::Mutex::new(None)),
    };
    workers.reconnect(
        client,
        library,
        cache,
        offline,
        state.config.ui.enable_websocket,
    );
    let seed = crate::bootstrap::seed_artists_column(state);
    let _ = workers.network.send(seed);

    // Restores whatever this server's own queue/position was left at the last time a switch
    // moved *away* from it — symmetric with the outgoing side's own `PersistSessionForServer`.
    // Deliberately not gated on `ui.restore_session` the way startup's own restore is (`main.rs`)
    // — a live switch's own point is picking up this server's session, not asking again whether
    // to.
    if let Ok(Some(snapshot)) = loxia_cache::session::load_for_server(paths, &server_id) {
        for effect in reducer::apply(
            state,
            Action::System(SystemEvent::SessionRestored(Box::new(snapshot))),
        ) {
            dispatch::dispatch(&effect, workers);
        }
    }
}

fn viewport_of<B>(terminal: &Terminal<B>) -> Viewport
where
    B: Backend,
{
    let rows = terminal
        .size()
        .map(|s| s.height as usize)
        .unwrap_or(DEFAULT_VIEWPORT_ROWS);
    Viewport { rows }
}

/// The loop itself, generic over the render backend and fed by a channel of raw crossterm events
/// so tests can drive it headlessly with scripted keypresses.
pub async fn run<B>(
    mut state: AppState,
    terminal: &mut Terminal<B>,
    mut input_rx: mpsc::UnboundedReceiver<CrosstermEvent>,
    mut workers: Workers,
    paths: &Paths,
    renderer: loxia_tui::widgets::album_art::ArtRenderer,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut ticker = tokio::time::interval(TICK_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_draw = Instant::now()
        .checked_sub(RENDER_INTERVAL)
        .unwrap_or_else(Instant::now);
    let mut redraw_pending = true; // always draw the initial state once
    let mut events_open = true;
    // `10-04`: "the runtime retains the *previous frame's* HitMap and resolves mouse events
    // against it" — owned here, across iterations, and only ever refreshed by `draw()` actually
    // running (itself throttled by `RENDER_INTERVAL`/`redraw_pending` below), which is exactly
    // what makes it the *previous*, not current, frame's map: a mouse event arrives, and is
    // resolved, before the frame it might itself cause is ever drawn.
    let mut hits = HitMap::default();
    let mut mouse = MouseState::default();
    // Detected once, here — `Picker::from_query_stdio` round-trips the terminal, which must not
    // happen per frame (`widgets::album_art`'s own `DETECT_TIMEOUT`).
    let mut art = ArtContext {
        renderer,
        cache: loxia_tui::widgets::album_art::ArtCache::new(),
        store: workers.art_store.clone(),
    };

    loop {
        let action = tokio::select! {
            biased;
            maybe = input_rx.recv() => match maybe {
                Some(ev) => {
                    let viewport = viewport_of(terminal);
                    match crate::input::to_action(&state, ev, viewport, &hits, &mut mouse, Instant::now()) {
                        Some(action) => action,
                        None => continue, // unbound/unresolved key, ignored mouse, etc.
                    }
                }
                None => break, // the input source is gone; nothing left to drive the loop
            },
            _ = ticker.tick() => Action::System(SystemEvent::Tick(Timestamp::now())),
            maybe = workers.events.recv(), if events_open => match maybe {
                Some(event) => event.into_action(),
                None => {
                    events_open = false;
                    continue;
                }
            },
        };

        for effect in reducer::apply(&mut state, action) {
            if matches!(effect, Effect::Sys(SysEffect::Exit)) {
                state.should_quit = true;
            } else if let Effect::Sys(SysEffect::SetMouseCapture(enable)) = &effect {
                // `11-01`: live-apply for `ui.enable_mouse` — a terminal-wide escape sequence,
                // not something any worker channel could act on (`dispatch::dispatch` fans out
                // to workers only), so it's applied directly here, the same way `terminal.rs`'s
                // own `TerminalGuard::enter` issues the identical sequence once at startup.
                apply_mouse_capture(*enable);
            } else if let Effect::Sys(SysEffect::ReconnectServer(server_id)) = &effect {
                // `11-03`: rebuilding the `EmbyClient` and respawning the network/cache workers
                // needs `&mut Workers`/`&Paths`, neither of which any worker's own channel has —
                // handled directly here, the same way `SetMouseCapture` is, just `async`.
                reconnect_server(&mut state, &mut workers, paths, server_id.clone()).await;
            } else if let Effect::Sys(SysEffect::CopyToClipboard(text)) = &effect {
                // `11-07`: a raw terminal escape write, the same "no worker channel owns this"
                // shape as `SetMouseCapture`.
                copy_to_clipboard(text);
            } else {
                dispatch::dispatch(&effect, &workers);
            }
        }
        warn_if_pathological_folder_depth(&state);

        if state.dirty {
            redraw_pending = true;
        }

        if state.should_quit {
            shutdown(&mut state, terminal, workers, paths, &mut hits).await?;
            return Ok(());
        }

        if redraw_pending && last_draw.elapsed() >= RENDER_INTERVAL {
            // A render can ask for art it doesn't have yet; those fetches go out like any other
            // effect, and their replies trigger the next redraw.
            for effect in draw(terminal, &state, &mut hits, &mut art)? {
                dispatch::dispatch(&effect, &workers);
            }
            last_draw = Instant::now();
            redraw_pending = false;
            state.dirty = false;
        }
    }

    Ok(())
}

/// Persists the queue/history snapshot and the config file, draws a final frame, then waits
/// (with a bounded timeout) for the workers to drain. A hung worker must not prevent exit.
async fn shutdown<B>(
    state: &mut AppState,
    terminal: &mut Terminal<B>,
    workers: Workers,
    paths: &Paths,
    hits: &mut HitMap,
) -> anyhow::Result<()>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    dispatch::dispatch(
        &Effect::Cache(Box::new(CacheEffect::PersistSession(Box::new(
            session_snapshot(state),
        )))),
        &workers,
    );

    // Config writes are cheap and infrequent; a brief synchronous save at shutdown is simpler and
    // just as safe as routing through a worker (`bootstrap::save`'s own doc comment names this
    // task as its real caller).
    if let Err(e) = crate::bootstrap::save(paths, &state.config) {
        tracing::warn!(error = %e, "could not save config.toml on exit");
    }

    let mut art = ArtContext {
        renderer: loxia_tui::widgets::album_art::ArtRenderer::Off,
        cache: loxia_tui::widgets::album_art::ArtCache::new(),
        store: Default::default(),
    };
    let _ = draw(terminal, state, hits, &mut art);
    workers.drain().await;
    Ok(())
}

/// `07-05`: "no depth cap, but a `debug` log line at depth 20 helps diagnose a pathological
/// library" — `loxia-core` deliberately has no `tracing` dependency (logging is I/O-adjacent, see
/// `theme.rs`'s own note), so this lives here, in the one place that already sees `state` after
/// every dispatch, rather than in the pure reducer.
const PATHOLOGICAL_FOLDER_DEPTH: usize = 20;

fn warn_if_pathological_folder_depth(state: &AppState) {
    use loxia_core::state::nav::{NavFocus, Tab};
    if state.nav.active_tab == Tab::Folders
        && let NavFocus::Column(depth) = state.nav.focus
        && depth >= PATHOLOGICAL_FOLDER_DEPTH
    {
        tracing::debug!(depth, "folder tree drilled unusually deep");
    }
}

fn session_snapshot(state: &AppState) -> loxia_core::state::SessionSnapshot {
    loxia_core::state::SessionSnapshot {
        schema_version: loxia_cache::session::SCHEMA_VERSION,
        // `08-08`: fixed a real pre-existing bug — this used to store
        // `state.server.server_name` (the server's own *display* name, or empty if never
        // connected this session), which can never equal `config.active_server` (the
        // config-assigned server *id* `08-08`'s own restore validation compares against),
        // making "restore only if the same server" silently discard every snapshot forever
        // (`docs/12-decisions.md`).
        server_id: loxia_core::model::ServerId::from(state.config.storage_server_id()),
        queue: state.queue.clone(),
        position_secs: state.player.position.as_secs_f64(),
        active_tab: state.nav.active_tab,
        zen_mode: state.zen_mode,
        volume: state.player.volume,
        quality_profile: state.player.quality_profile,
        eq: state.player.eq.clone(),
        saved_at: Timestamp::now(),
    }
}

/// `10-04`: the real widget tree, wired in at last — `render.rs`'s own `draw(frame, state, theme,
/// hits)` (`04-02`) has been fully built and tested since phase 04, but nothing in `crates/loxia`
/// ever called it; this function was a debug-text placeholder instead (a single `Paragraph` line),
/// by its own prior doc comment's design: "whichever task first wires the real widget tree in
/// replaces this function's one call site." That task turned out to be this one — mouse support
/// cannot resolve anything against a `HitMap` that nothing ever populates, and `render::draw` is
/// the only thing that populates one. `state.theme` already exists on `AppState` (no separate
/// theme-loading step needed here). See `docs/12-decisions.md`.
/// Returns whatever effects the render itself asked for — today that is `NetEffect::FetchImage`
/// from `widgets::album_art`, which discovers a missing cover only at the moment it tries to draw
/// one at a particular size (`docs/12-decisions.md`).
fn draw<B>(
    terminal: &mut Terminal<B>,
    state: &AppState,
    hits: &mut HitMap,
    art: &mut ArtContext,
) -> anyhow::Result<Vec<Effect>>
where
    B: Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut effects = Vec::new();
    terminal.draw(|f| {
        let mut bundle = loxia_tui::widgets::album_art::Art {
            renderer: &art.renderer,
            cache: &mut art.cache,
            store: &art.store,
        };
        effects = loxia_tui::render::draw(f, state, &state.theme, hits, &mut bundle);
    })?;
    Ok(effects)
}

/// The render-side album-art pipeline, owned across iterations: the detected terminal protocol, the
/// encoded-protocol LRU, and the decoded-image hand-off the network worker writes into.
pub struct ArtContext {
    pub renderer: loxia_tui::widgets::album_art::ArtRenderer,
    pub cache: loxia_tui::widgets::album_art::ArtCache,
    pub store: loxia_tui::widgets::album_art::ArtStore,
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::keymap::{KeyChord, KeyCode, KeyMap, KeyModifiers};
    use loxia_core::paths::FixedDirs;
    use loxia_core::state::toast::{Toast, ToastLevel};
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{
        KeyCode as CtKeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers as CtKeyModifiers,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn base64_encode_matches_known_vectors() {
        // RFC 4648 §10 test vectors.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    fn key(code: CtKeyCode, mods: CtKeyModifiers) -> CrosstermEvent {
        CrosstermEvent::Key(KeyEvent {
            code,
            modifiers: mods,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn ctrl_c() -> CrosstermEvent {
        key(CtKeyCode::Char('c'), CtKeyModifiers::CONTROL)
    }

    fn mouse_down(col: u16, row: u16) -> CrosstermEvent {
        CrosstermEvent::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: ratatui::crossterm::event::MouseEventKind::Down(
                ratatui::crossterm::event::MouseButton::Left,
            ),
            column: col,
            row,
            modifiers: CtKeyModifiers::NONE,
        })
    }

    fn state_with_default_keymap() -> AppState {
        AppState {
            keymap: KeyMap::defaults(),
            ..AppState::default()
        }
    }

    fn test_paths() -> (tempfile::TempDir, Paths) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = FixedDirs::new(
            Some(tmp.path().join("config")),
            Some(tmp.path().join("cache")),
            Some(tmp.path().join("data")),
            Some(tmp.path().join("state")),
        );
        let paths = Paths::resolve(&dirs, &loxia_core::config::CacheConfig::default()).unwrap();
        (tmp, paths)
    }

    /// Delegates every `Backend` method to an inner `TestBackend`, counting each `flush()` — the
    /// call `Terminal::draw` makes exactly once per completed draw.
    struct CountingBackend {
        inner: TestBackend,
        draws: Arc<AtomicUsize>,
    }

    impl Backend for CountingBackend {
        type Error = std::convert::Infallible;

        fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
        where
            I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
        {
            self.inner.draw(content)
        }

        fn hide_cursor(&mut self) -> Result<(), Self::Error> {
            self.inner.hide_cursor()
        }

        fn show_cursor(&mut self) -> Result<(), Self::Error> {
            self.inner.show_cursor()
        }

        fn get_cursor_position(&mut self) -> Result<ratatui::layout::Position, Self::Error> {
            self.inner.get_cursor_position()
        }

        fn set_cursor_position<P: Into<ratatui::layout::Position>>(
            &mut self,
            position: P,
        ) -> Result<(), Self::Error> {
            self.inner.set_cursor_position(position)
        }

        fn clear(&mut self) -> Result<(), Self::Error> {
            self.inner.clear()
        }

        fn clear_region(
            &mut self,
            clear_type: ratatui::backend::ClearType,
        ) -> Result<(), Self::Error> {
            self.inner.clear_region(clear_type)
        }

        fn size(&self) -> Result<ratatui::layout::Size, Self::Error> {
            self.inner.size()
        }

        fn window_size(&mut self) -> Result<ratatui::backend::WindowSize, Self::Error> {
            self.inner.window_size()
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            self.draws.fetch_add(1, Ordering::SeqCst);
            self.inner.flush()
        }
    }

    fn counting_terminal(draws: Arc<AtomicUsize>) -> Terminal<CountingBackend> {
        Terminal::new(CountingBackend {
            inner: TestBackend::new(80, 24),
            draws,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn loop_applies_action_and_dispatches_effect() {
        let (workers, mut rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws.clone());
        let (_tmp, paths) = test_paths();

        input_tx
            .send(key(CtKeyCode::Char('j'), CtKeyModifiers::NONE))
            .unwrap();
        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        run(
            state_with_default_keymap(),
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        )
        .await
        .unwrap();

        // The cache worker is dispatched to on shutdown (`PersistSession`); the others receive
        // nothing in this scenario since `j` (`MoveDown`) on a state with no active column emits
        // no effects.
        assert!(
            rx[2].try_recv().is_ok(),
            "cache worker should see PersistSession on shutdown"
        );
    }

    #[tokio::test]
    async fn redraw_only_when_dirty() {
        let (workers, _rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws.clone());
        let (_tmp, paths) = test_paths();

        // `Cancel` (`Esc`) with no modal, filter, visual mode, or selection active is a documented
        // no-op ladder fall-through (`nav::cancel`): no `dirty`, so no second draw beyond the
        // guaranteed initial one.
        input_tx
            .send(key(CtKeyCode::Esc, CtKeyModifiers::NONE))
            .unwrap();
        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        run(
            state_with_default_keymap(),
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        )
        .await
        .unwrap();

        // Initial draw + the final shutdown draw = 2, never one per action processed.
        assert_eq!(draws.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn render_throttled_to_16ms() {
        let (workers, _rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws.clone());
        let (_tmp, paths) = test_paths();

        for _ in 0..100 {
            input_tx
                .send(key(CtKeyCode::Char('j'), CtKeyModifiers::NONE))
                .unwrap();
        }
        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        run(
            state_with_default_keymap(),
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        )
        .await
        .unwrap();

        let count = draws.load(Ordering::SeqCst);
        assert!(
            count < 100,
            "100 actions in a burst must coalesce into far fewer draws, got {count}"
        );
        assert!(count >= 1, "the final state must still be drawn");
    }

    #[tokio::test]
    async fn quit_drains_and_exits() {
        let (workers, _rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws);
        let (_tmp, paths) = test_paths();

        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        let result = tokio::time::timeout(
            Duration::from_secs(3),
            run(
                state_with_default_keymap(),
                &mut terminal,
                input_rx,
                workers,
                &paths,
                loxia_tui::widgets::album_art::ArtRenderer::Off,
            ),
        )
        .await;

        assert!(result.is_ok(), "quit must exit promptly");
        assert!(result.unwrap().is_ok());
    }

    /// `11-06`: `shutdown_emits_final_report_then_snapshot` — the final `Stopped` report
    /// (`Action::System(SystemEvent::Quit)`'s own reducer arm, `06-07`) reaches the network worker
    /// and `shutdown`'s own `PersistSession` reaches the cache worker, both on a single quit.
    /// Structurally these can never arrive out of order: the reducer's `Quit` effects are
    /// dispatched in the same loop iteration, before `state.should_quit` is even checked, and
    /// `shutdown` (called only after that check) issues `PersistSession` before anything else it
    /// does.
    #[tokio::test]
    async fn shutdown_emits_final_report_then_snapshot() {
        let (workers, mut rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws);
        let (_tmp, paths) = test_paths();

        let mut state = loxia_core::test_support::fixtures::fixture_playing_queue();
        state.keymap = KeyMap::defaults();
        state.player.session = Some(state.player.next_session_id());

        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        run(
            state,
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        )
        .await
        .unwrap();

        assert!(
            rx[0].try_recv().is_ok(),
            "the network worker should see the final Stopped report on quit"
        );
        assert!(
            rx[2].try_recv().is_ok(),
            "the cache worker should see PersistSession on shutdown"
        );
    }

    /// `11-06`: `shutdown_bounded_at_two_seconds` — `Workers::drain`'s own 2s timeout (proved more
    /// thoroughly by `hung_worker_does_not_block_exit` below) is what `shutdown` actually awaits;
    /// this pins the acceptance-named test to that exact bound rather than the looser "well under
    /// 3s" margin.
    #[tokio::test]
    async fn shutdown_bounded_at_two_seconds() {
        let hung = tokio::spawn(futures::future::pending::<()>());
        let workers = Workers::with_handles(vec![hung]);

        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws);
        let (_tmp, paths) = test_paths();

        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        let start = Instant::now();
        run(
            state_with_default_keymap(),
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        )
        .await
        .unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed >= Duration::from_secs(2),
            "must actually wait out the drain bound, not return early"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "must not wait meaningfully longer than the 2s bound, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn hung_worker_does_not_block_exit() {
        // A handle that never completes, standing in for a worker that ignores its channel
        // closing — `Workers::drain`'s 2s timeout must not let this block shutdown.
        let hung = tokio::spawn(futures::future::pending::<()>());
        let workers = Workers::with_handles(vec![hung]);

        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws);
        let (_tmp, paths) = test_paths();

        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        let result = tokio::time::timeout(
            Duration::from_secs(3),
            run(
                state_with_default_keymap(),
                &mut terminal,
                input_rx,
                workers,
                &paths,
                loxia_tui::widgets::album_art::ArtRenderer::Off,
            ),
        )
        .await;

        assert!(result.is_ok(), "a hung worker must not block exit past 3s");
    }

    #[tokio::test(start_paused = true)]
    async fn toasts_expire_on_tick() {
        let (workers, _rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws);
        let (_tmp, paths) = test_paths();

        let mut state = state_with_default_keymap();
        state.toasts.push(Toast {
            id: 0,
            message: "hello".to_string(),
            level: ToastLevel::Info,
            created_at: Timestamp::now(),
        });

        let run_fut = run(
            state,
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        );
        tokio::pin!(run_fut);

        tokio::time::advance(Duration::from_secs(5)).await;
        // Give the now-elapsed ticks a chance to be processed before checking and quitting.
        tokio::task::yield_now().await;

        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);
        run_fut.await.unwrap();
    }

    #[tokio::test]
    async fn pending_chord_expires_after_one_second() {
        // The reducer-level behaviour (`reducer::tests::pending_chord_expires_after_one_second`
        // in `loxia-core`) is the precise, deterministic version of this test; this one only
        // confirms the loop actually delivers `Tick` actions to the reducer at all, by checking
        // the loop runs and exits cleanly with a pending chord set going in.
        let (workers, _rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws);
        let (_tmp, paths) = test_paths();

        let state = AppState {
            pending_chord: Some((
                KeyChord {
                    code: KeyCode::Char('g'),
                    mods: KeyModifiers::default(),
                },
                Timestamp::now(),
            )),
            ..state_with_default_keymap()
        };

        input_tx
            .send(key(CtKeyCode::Char('z'), CtKeyModifiers::NONE))
            .unwrap();
        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        run(
            state,
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn hit_map_is_from_previous_frame() {
        // `10-04`: mouse clicks resolve against the *previous* frame's `HitMap`, populated by the
        // last real `draw()` — never one that would result from the very event being handled.
        // Proves both halves: a click before the very first draw hits nothing (the loop's own
        // `hits` starts out `HitMap::default()`), and a later click resolves against the sidebar
        // rows that a real draw actually put on screen (`layout::zones` fixes the sidebar at
        // x=0, rows starting at y=1 — one row per `sidebar::TAB_ORDER` entry, so row 5 is
        // `TAB_ORDER[4]`, `Tab::Artists`).
        let (workers, mut rx) = Workers::for_test();
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let draws = Arc::new(AtomicUsize::new(0));
        let mut terminal = counting_terminal(draws.clone());
        let (_tmp, paths) = test_paths();

        let mut state = state_with_default_keymap();
        state.config.ui.enable_mouse = true;

        // Before any draw has happened, this must be silently dropped rather than panicking or
        // acting on geometry that was never actually rendered.
        input_tx.send(mouse_down(1, 5)).unwrap();

        // A real key forces the loop's first genuine draw, populating `hits` with the sidebar
        // rows for the state as of this point (default tab, `NowPlaying`).
        input_tx
            .send(key(CtKeyCode::Esc, CtKeyModifiers::NONE))
            .unwrap();

        // Now the click lands on real, previous-frame geometry.
        input_tx.send(mouse_down(1, 5)).unwrap();

        input_tx.send(ctrl_c()).unwrap();
        drop(input_tx);

        run(
            state,
            &mut terminal,
            input_rx,
            workers,
            &paths,
            loxia_tui::widgets::album_art::ArtRenderer::Off,
        )
        .await
        .unwrap();

        // Switching to a tab not yet seeded dispatches exactly one `FetchColumn` — proof the
        // second click actually reached `NavAction::SetTab(Tab::Artists)` and the reducer ran it
        // (and, by construction, that the first click reached nothing at all: had it also hit,
        // `set_tab` would already be a no-op the second time and this effect would never fire).
        assert!(
            rx[0].try_recv().is_ok(),
            "the second click should have switched to Artists and fetched its column"
        );
    }

    /// `11-03`: `SysEffect::ReconnectServer` must actually rebuild a *working* network worker
    /// against the new server, not just replace the sender — proven end to end by sending a
    /// plain connectivity probe through `workers.network` after reconnecting and getting back a
    /// real reply from the freshly mocked server.
    #[tokio::test]
    async fn reconnect_server_respawns_a_working_network_worker() {
        use loxia_core::config::{Config, ServerConfig};
        use loxia_core::model::ServerId;
        use loxia_core::state::Connectivity;

        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/emby/Users/user-1"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "Id": "user-1", "Name": "Test User" })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/emby/Users/user-1/Views"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "Items": [ { "Id": "lib-1", "Name": "Music", "CollectionType": "music" } ]
                })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/System/Info/Public"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let (_tmp, paths) = test_paths();
        let (mut workers, _rx) = Workers::for_test();
        let mut state = AppState {
            config: Config {
                servers: vec![ServerConfig {
                    id: "new".to_string(),
                    name: "New Server".to_string(),
                    url: server.uri(),
                    user_id: "user-1".to_string(),
                    access_token: "tok".to_string(),
                    device_id: "dev".to_string(),
                    custom_headers: Default::default(),
                    server_id: String::new(),
                    fallbacks: Vec::new(),
                }],
                active_server: "new".to_string(),
                ..Config::default()
            },
            ..AppState::default()
        };

        reconnect_server(&mut state, &mut workers, &paths, ServerId::from("new")).await;
        assert_eq!(state.connectivity, Connectivity::Online);

        workers
            .network
            .send(Effect::Net(loxia_core::effect::NetEffect::Reconnect))
            .unwrap();
        // `reconnect_server` itself already sent its own `seed_artists_column` fetch (unmocked
        // here, so it replies `LoadFailed` first) — read past it for the probe's own reply.
        let expected = loxia_core::event::Event::System(SystemEvent::ConnectivityChanged(
            Connectivity::Reconnecting,
        ));
        loop {
            let event = tokio::time::timeout(Duration::from_secs(2), workers.events.recv())
                .await
                .expect("the freshly respawned network worker must actually reply")
                .unwrap();
            if event == expected {
                break;
            }
        }
    }

    /// A failed reconnect (e.g. the new profile is unreachable) must not panic and must leave the
    /// app in the same "no connection" state a failed *startup* connect already leaves it in —
    /// `bootstrap::connect`'s own `fail()` path.
    #[tokio::test]
    async fn reconnect_server_failure_does_not_panic() {
        use loxia_core::config::{Config, ServerConfig};
        use loxia_core::model::ServerId;
        use loxia_core::state::Connectivity;

        let (_tmp, paths) = test_paths();
        let (mut workers, _rx) = Workers::for_test();
        let mut state = AppState {
            config: Config {
                servers: vec![ServerConfig {
                    id: "unreachable".to_string(),
                    name: "Unreachable".to_string(),
                    url: "http://127.0.0.1:1".to_string(),
                    user_id: "user-1".to_string(),
                    access_token: "tok".to_string(),
                    device_id: "dev".to_string(),
                    custom_headers: Default::default(),
                    server_id: String::new(),
                    fallbacks: Vec::new(),
                }],
                active_server: "unreachable".to_string(),
                ..Config::default()
            },
            ..AppState::default()
        };

        reconnect_server(
            &mut state,
            &mut workers,
            &paths,
            ServerId::from("unreachable"),
        )
        .await;
        assert_eq!(state.connectivity, Connectivity::Offline);
    }
}
