# 04 — State Machine, Actions & Input

This is the heart of the application. Get this right and the rest is mechanical.

## 1. The three enums

```
        KeyEvent / MouseEvent
                 │  keymap::resolve(context, chord)
                 ▼
             ┌────────┐   reducer::apply(&mut AppState, Action)   ┌────────┐
             │ Action │ ─────────────────────────────────────────►│ Effect │
             └────────┘                                           └────────┘
                 ▲                                                     │
                 │  Event::into_action()                               │ dispatch
             ┌────────┐                                                ▼
             │ Event  │◄────────────────────────────────────── workers (net/audio/cache)
             └────────┘
```

- **`Action`** — an intent. The only thing that mutates state. Serializable, which enables
  record-and-replay tests.
- **`Effect`** — a request for I/O. Plain data; never contains a closure or a channel handle.
- **`Event`** — a result from the outside world, converted 1:1 into an `Action` by the runtime.

**Why workers never touch state:** if they did, the app could only be tested by running it. With
this split a test is `apply(&mut state, action)` followed by assertions on `state` and on the
returned `Vec<Effect>`. Every behavioural requirement becomes a unit test with zero mocks.

## 2. `Action` taxonomy (`action.rs`)

Nested enums keep the match arms manageable.

| Group | Variants |
| :-- | :-- |
| `Action::Nav` | `MoveUp{n}`, `MoveDown{n}`, `NavLeft`, `NavRight`, `HalfPageUp`, `HalfPageDown`, `GoToTop`, `GoToBottom`, `PopColumn`, `SetTab(Tab)`, `NextTab`, `PrevTab`, `GoToArtist`, `GoToAlbum`, `OpenFilter`, `SetFilter(String)`, `Cancel` |
| `Action::Select` | `ToggleVisualMode`, `ToggleItem`, `SelectAll`, `ClearSelection` |
| `Action::Queue` | `QueueSelection{full_context: bool}`, `InsertNext`, `RemoveEntry`, `MoveEntry{from,to}`, `Clear`, `ToggleShuffle{seed: u64}` (seed supplied by the input layer from `state.clock`, `06-03` — never read from a clock inside the reducer), `CycleRepeat`, `ApplySortProfile(String)`, `InstantMix`, `JumpTo(QueueEntryId)`, `RequeueTrack(Box<Track>)` (`07-06`: `a` on a Now Playing History row) |
| `Action::Player` | `PlayPause`, `Stop`, `Next`, `Prev`, `Seek(SeekTarget)`, `SetVolume(u8)`, `VolumeDelta(i8)`, `ToggleMute`, `CycleQuality`, `CycleReplayGain`, `SetDevice(String)`, `SetEqGain{band,db}`, `SetEqPreset(String)`, `ToggleEqBypass` |
| `Action::Modal` | `Open(ModalKind)`, `Close`, `Submit`, `FieldNext`, `FieldPrev`, `FieldInput(char)`, `FieldBackspace` |
| `Action::View` | `ToggleZen`, `ToggleLyrics`, `ToggleHelp`, `ToggleHistory`, `SetTheme(String)` |
| `Action::Item` | `ToggleFavorite`, `ToggleDownload`, `AddToPlaylist{..}`, `RemoveFromPlaylist{playlist, entries}` (`07-03`: plural `entries`, multi-select removes them in one request), `MoveInPlaylist{playlist, entry, new_index}` (`07-03`), `DeletePlaylist(PlaylistId)` (opens `Confirm`; `DeletePlaylistConfirmed(PlaylistId)` is its `on_confirm` target, never bound to a key), `SaveQueueAsPlaylist{..}` |
| `Action::Data` | `ItemsLoaded{..}`, `TracksLoaded{..}`, `DiscographyLoaded{..}`, `LyricsLoaded{..}`, `ImageLoaded{..}`, `DevicesLoaded{..}`, `CacheResolved{track, profile, path: Option<PathBuf>}` (`08-03`), `LoadFailed{..}` |
| `Action::Audio` | `StatusChanged(PlayStatus)`, `PositionChanged{..}`, `TrackEnded{natural}`, `FormatDetected(AudioFormat)`, `EngineError(String)` |
| `Action::System` | `Tick(Timestamp)`, `Resize{w,h}`, `Refresh`, `Quit`, `Toast(Toast)`, `ConnectivityChanged(Connectivity)`, `SessionRestored(..)`, `ConfigChanged(Config)` |

## 3. `Effect` taxonomy (`effect.rs`)

| Group | Variants |
| :-- | :-- |
| `Effect::Net` | `FetchColumn{kind, page}`, `FetchDiscography{artist}` (now two album-level requests — `AlbumArtistIds` for primary, `ArtistIds` for all, diffed client-side — see `03-emby-api.md` §4), `FetchAlbumTracks{album, filter}` (column population), `FetchAlbumTracksForQueue{album}`/`FetchArtistTracksForQueue{artist}` (`06-02`'s `a`/`A` queue rules — always unfiltered; the `ArtistOnly` filter is applied client-side once the reply lands, never baked into the request), `Search{query}`, `InstantMix{seed, limit}` (`06-08`: `limit` is always `100`), `SetFavorite{id,on}`, `PlaylistCreate/SetOverview/Add/Remove/Delete/Move`, `ReportPlayback(PlaybackReport)`, `FetchLyrics{track, stream_ref}`, `FetchImage{id,size,tag}`, `Reconnect` |
| `Effect::Audio` | `Load{url, headers, start_at, gain_db}`, `Preload{url, headers, gain_db}`, `PlayPause`, `Stop`, `Seek(SeekTarget)`, `SetVolume(u8)`, `SetMute(bool)`, `SetEq(Option<EqCurve>)`, `SetReplayGain(Mode)`, `SetDevice(String)`, `EnumerateDevices` |
| `Effect::Cache` | `EnsureCached{track, profile}`, `PinDownload{scope}`, `RemoveDownload{id}`, `PersistSession(SessionSnapshot)`, `AppendHistory(HistoryEntry)`, `AppendScrobble(..)`, `DrainScrobbles` |
| `Effect::Sys` | `Notify(TrackChange)`, `UpdateMpris(MprisMeta)`, `WriteConfig(Config)`, `Exit` |

## 4. Reducer rules

Signature: `pub fn apply(state: &mut AppState, action: Action) -> Vec<Effect>`

1. **Total and panic-free.** Every index is clamped, every `Option` handled. A reducer panic crashes
   the app with the terminal in raw mode — unacceptable.
2. **No time, no randomness, no I/O.** `Tick` carries the timestamp. Shuffle takes a seed from the
   action payload. Tests are therefore fully deterministic.
3. **Idempotent where it can be.** `SetTab(Artists)` twice is a no-op the second time.
4. **Sets `state.dirty = true`** whenever anything visible changed, via the `state.touch()` helper.
5. **Optimistic with rollback.** `ToggleFavorite` flips the local flag immediately and emits the
   effect; a matching `LoadFailed` flips it back and raises a toast. Same for download pins and
   playlist mutations.
6. **Never claims playback.** `PlayerState` changes only in response to `Action::Audio(..)`. Pressing
   `Space` emits `Effect::Audio(PlayPause)` and changes nothing else; the engine's reply updates the
   mirror. This keeps the UI honest when the engine stalls.

| Module | Handles |
| :-- | :-- |
| `reducer::nav` | `Nav`, `Select`, and the column-loading half of `Data` |
| `reducer::queue` | `Queue`, `Item`, and the queue-append half of `Data` |
| `reducer::player` | `Player`, `Audio`, and sleep-timer evaluation on `Tick` |
| `reducer::modal` | `Modal`, plus modal-first input interception |
| `reducer::settings` | `View::SetTheme`, `System::ConfigChanged`, keymap edits |

## 5. Input contexts

The default keymap is deliberately **flat** — one chord means one action everywhere. Contexts exist
only to carve out two exclusive modes:

| Context | Rule |
| :-- | :-- |
| `TextInput` | A text field is focused (inline filter, playlist name, keymap capture). Every printable key goes to the buffer. Only `Esc`, `Enter`, `Tab`, `Shift+Tab`, and the arrows are bindable. |
| `Modal(kind)` | A modal owns the keyboard. Its own table applies, plus `Esc` (close), `?` (help), and `Ctrl+C` (quit). |
| `Normal` | Everything else. The full default table below applies unchanged in every view. |

Because `Normal` is a single flat table, **no chord can ever mean two things**, and the help modal
and inline key hints never need context qualifiers.

Actions whose *target* depends on focus — `NavLeft`, `MoveDown`, `RemoveEntry`, `ToggleFavorite` —
remain one action each. The reducer resolves the target from `nav.focus`. This is target resolution,
not an overloaded binding.

## 6. Default keymap (`keymap/defaults.rs`)

This table is normative. `KeyMap::validate()` must report zero conflicts for it, and a test asserts
exactly that.

### Navigation
| Chord | ActionId | Description |
| :-- | :-- | :-- |
| `j` / `↓` | `MoveDown` | Move down in the focused list |
| `k` / `↑` | `MoveUp` | Move up in the focused list |
| `h` / `←` | `NavLeft` | Move one column / pane left |
| `l` / `→` | `NavRight` | Drill into the selection / move right |
| `Ctrl+U` / `PageUp` | `HalfPageUp` | Up half a screen |
| `Ctrl+D` / `PageDown` | `HalfPageDown` | Down half a screen |
| `g` `g` / `Home` | `GoToTop` | Jump to the first item |
| `G` / `End` | `GoToBottom` | Jump to the last item |
| `Backspace` | `PopColumn` | Step out one Miller level |
| `Tab` / `Shift+Tab` | `NextTab` / `PrevTab` | Cycle sidebar tabs — **without exception**, including on Settings and from the Search query line |
| `Alt+1`…`Alt+9`, `Alt+0`, `F2`…`F10` | `JumpTab1`…`JumpTab10` | Jump straight to a tab. `Alt+0`/`F10` is the tenth (Settings) — the digits run out at nine. `F2`…`F9` mirror `Alt+2`…`Alt+9`; `F1` is help |
| `/` | `OpenFilter` | Inline fuzzy filter for the active column |
| `g` `a` | `GoToArtist` | Jump to the playing track's artist |
| `g` `l` | `GoToAlbum` | Jump to the playing track's album |
| `Esc` | `Cancel` | Clear filter → clear selection → exit visual mode → close modal |

### Playback and seeking
| Chord | ActionId | Description |
| :-- | :-- | :-- |
| `Space` | `PlayPause` | Toggle play / pause |
| `n` | `NextTrack` | Next track |
| `p` | `PrevTrack` | Previous track |
| `S` | `Stop` | Stop playback |
| `[` / `]` | `SeekBack5` / `SeekForward5` | Seek ∓5 s |
| `{` / `}` | `SeekBack30` / `SeekForward30` | Seek ∓30 s |
| `+` / `=` | `VolumeUp` | Volume +5 |
| `-` / `_` | `VolumeDown` | Volume −5 |
| `M` | `ToggleMute` | Mute / unmute |

### Queue and selection
| Chord | ActionId | Description |
| :-- | :-- | :-- |
| `Enter` / `a` | `QueueArtistOnly` | Queue selection, artist-filtered on compilations |
| `A` / `Shift+Enter` | `QueueFullContext` | Queue the complete album / compilation |
| `i` | `InsertNext` | Insert selection as play-next |
| `m` | `InstantMix` | Generate an instant mix from the selection |
| `s` | `ToggleShuffle` | Non-destructive shuffle / unshuffle |
| `R` | `CycleRepeat` | Off → All → One |
| `o` | `OpenSortMenu` | Sort profile picker |
| `x` | `RemoveEntry` | Remove the focused queue entry or playlist track |
| `v` | `ToggleVisualSelect` | Enter / leave visual multi-select |
| `.` | `ToggleItem` | Toggle selection of the focused item |
| `V` | `SelectAll` | Select every item in the column |

### Audio and DSP
| Chord | ActionId | Description |
| :-- | :-- | :-- |
| `e` | `ToggleEqualizer` | 10-band equalizer modal |
| `r` | `CycleReplayGain` | Album → Track → Off |
| `q` | `CycleQualityProfile` | Direct → 320k → 192k → 96k |
| `O` | `OpenDevicePicker` | Audio output device switcher |
| `T` | `OpenSleepTimer` | Sleep timer modal |

### Items, playlists and views
| Chord | ActionId | Description |
| :-- | :-- | :-- |
| `f` | `ToggleFavorite` | Favourite / unfavourite the selection |
| `d` | `ToggleDownload` | Pin / unpin for offline |
| `P` | `SaveQueueAsPlaylist` | Save the queue as a playlist |
| `Ctrl+P` | `AddToPlaylist` | Add the selection to a playlist |
| `X` | `DeletePlaylist` | Delete the focused playlist (confirmed) |
| `x` | `RemoveEntry` | Remove the focused queue entry; on the Playlists tab's `PlaylistTracks` column, removes the focused (or every selected) track from the playlist instead (`07-03`); on Now Playing's Queue sub-view, removes the row under `now_playing_cursor` instead of the currently-playing entry (`07-06`; a no-op on History, read-only) |
| `Ctrl+↑`/`Ctrl+↓` | `MoveTrackUp`/`MoveTrackDown` | Reorder the focused track within a `PlaylistTracks` column by one position (`07-03`); on Now Playing's Queue sub-view, reorders the queue itself instead (`07-06`) |
| `Enter` / `a` | `QueueArtistOnly` | Queue the selection everywhere except Now Playing (`07-06`), where it jumps to the focused queue row, or re-queues the focused history entry |
| `z` | `ToggleZenMode` | Zen focus view |
| `H` | `ToggleHistory` | Queue ⇄ History in Now Playing |
| `L` | `ToggleLyrics` | Show / hide the lyrics pane |
| `K` / `J` | `LyricsScrollUp` / `LyricsScrollDown` | Scroll **untimed** lyrics; timed ones follow playback |
| `?` / `F1` | `ToggleHelp` | Context help overlay |

### System
| Chord | ActionId | Description |
| :-- | :-- | :-- |
| `Ctrl+Q` / `Ctrl+C` | `Quit` | Save session and exit |
| `Ctrl+R` | `Refresh` | Reload the focused column / retry after an error |

### Notes on the design
- **Seek is on `[`/`]`, not `h`/`l`.** The original spec bound both to the same keys; moving seek to
  the brackets removes the conflict without context-scoping and keeps vim navigation intact.
- **Selection toggles on `.`, not `Space`.** `Space` stays play/pause everywhere, which is the more
  frequently used action and the one users expect to be unconditional.
- **`Shift+Enter` is an alias, never the advertised key.** Telling it apart from a bare `Enter`
  needs the terminal's keyboard-enhancement protocol, which loxia does not enable, so in most
  terminals the chord never arrives at all. `A` is what the UI advertises for `QueueFullContext`;
  `Shift+Enter` stays bound for the terminals that do report it.
- **`F2`–`F9` mirror `Alt+2`–`Alt+9`** because terminal emulators, tmux, and screen frequently
  intercept `Alt`-modified digits. `F1` is deliberately excluded: it is the help key in effectively
  every program a user has ever run, so it opens the help overlay and tab 1 keeps only `Alt+1`.
- **`1`–`5` are unbound.** They were star ratings, which Emby does not support.

## 7. Keymap implementation (`keymap/`)

### Chord model
`KeyChord { code: KeyCode, mods: KeyModifiers }`. A `KeyBinding` is `Vec<KeyChord>` of length 1 or 2
— length 2 supports the `g`-prefixed sequences. `AppState.pending_chord` holds a partial sequence
with a **1-second timeout** cleared on `Tick`; the pending prefix is echoed in the status bar.

### Binding string syntax (`keymap/parse.rs`)
Two syntaxes are accepted on read; the friendly one is always used on write.

| Friendly (canonical) | Verbose (also accepted) |
| :-- | :-- |
| `space`, `?`, `enter`, `esc`, `tab`, `backspace` | `Space`, `Char('?')`, `Enter` |
| `alt+1`, `ctrl+p`, `shift+enter` | `Modifiers(ALT) + Char('1')` |
| `g a` (space-separated sequence) | — |

`parse(render(b)) == b` for every binding, property-tested. Modifier order is normalised to
`ctrl+alt+shift+`. Parsing is case-insensitive for names, case-**sensitive** for bare characters —
`P` and `p` are different bindings.

### Conflict detection (`keymap/validate.rs`)
```
fn validate(&self) -> Vec<KeyConflict>
KeyConflict { context: InputContext, chord: KeyBinding, actions: Vec<ActionId> }
```
Runs on **every config load**, not just in the editor. Behaviour on conflict:

1. The **last binding wins** (config order), so the app always has a usable keymap.
2. Each conflict becomes a `ConfigWarning`, stored in `AppState.config_warnings`.
3. Three notifications, all required:
   - a **startup toast**: `⚠ 2 keybinding conflicts — see Settings → Keybindings`
   - a **badge and per-row detail** in the Settings keybinding list, showing the losing action
     greyed out beside the winner
   - a **`WARN` log line per conflict** naming the chord and every action bound to it
4. Conflicts **never abort startup**. A broken config must still open a working player.

The in-UI remapper (`11-settings`) refuses to commit a capture that would conflict, highlights the
incumbent action, and offers to unbind it instead.

## 8. Mouse handling (`loxia-tui/hit.rs` + `input.rs`)

During `draw()`, widgets push regions into a `HitMap`: `Vec<(Rect, HitTarget)>`, where `HitTarget`
is `SidebarTab(Tab) | ColumnItem{col, idx} | SeekBar | VolumeBar | Transport(TransportButton) |
ModalField(usize) | EqBand(usize) | QueueEntry(QueueEntryId)`.

The runtime retains the **previous frame's** `HitMap` and resolves mouse events against it:
- `Down(Left)` on `ColumnItem` → focus that column and set the cursor; a second click within 400 ms
  → `NavRight` (drill in)
- `Down(Left)` on `SeekBar` → `Seek(Fraction(x_rel))`
- `ScrollUp` / `ScrollDown` → `MoveUp{3}` / `MoveDown{3}` applied to the column **under the pointer**,
  not the focused one
- `Drag` on `SeekBar` or `EqBand` → continuous update, committed on release

Gated by `ui.enable_mouse`. When disabled, crossterm mouse capture is never enabled, so the
terminal's own text selection keeps working.

## 9. Tick responsibilities (100 ms)

On `Action::System::Tick`:
1. **Never extrapolate position** — it comes from the audio engine only.
2. Expire toasts and `pending_chord`.
3. Evaluate the sleep timer, including the 10 s fade-out ramp.
4. Every 10th tick (1 s): flush a debounced config write if dirty.
5. Every 100th tick (10 s): emit the playback progress report effect.
6. Every 150th tick (15 s): emit the session snapshot effect.

Everything periodic lives here. **No `tokio::spawn` with a sleep loop anywhere else in the app.**

## 10. Required reducer tests

Each maps to a behavioural requirement. All must exist by the end of phase 06.

```
section_headers_are_skipped_by_cursor_movement
drilling_past_three_columns_slides_window
pop_column_restores_previous_cursor
tab_switch_preserves_per_tab_column_stack
visual_selection_survives_filter_change
appears_on_enter_queues_only_artist_tracks
appears_on_shift_enter_queues_full_compilation
queue_full_context_preserves_compilation_order
shuffle_preserves_current_track
unshuffle_round_trips_to_original_order
sort_profile_applies_all_four_rules_in_order
repeat_one_replays_same_entry
auto_advance_stops_at_end_when_repeat_off
favorite_toggle_rolls_back_on_failure
play_pause_does_not_mutate_player_state_directly
quality_cycle_reloads_at_same_position
sleep_timer_end_of_queue_stops_after_last_track
offline_refuses_to_queue_unavailable_track
modal_open_replaces_existing_modal
pending_chord_expires_after_one_second
default_keymap_has_no_conflicts
keymap_conflict_reports_warning_and_last_binding_wins
```
