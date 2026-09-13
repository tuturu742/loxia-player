# 07 — UI Specification (`loxia-tui`)

## 1. Rendering contract

```
pub fn draw(f: &mut Frame, state: &AppState, theme: &Theme, hits: &mut HitMap)
```
Pure with respect to `AppState`. All layout derives from `f.area()` and state — no persistent widget
state, no interior mutability, no I/O. Modals draw last, over everything.

**Never hardcode a key in UI text.** Every hint renders through `state.keymap.hint_for(ActionId)`,
so remapping a key updates the inspector, the footers, and the help modal simultaneously.

## 2. Root layout

```
Constraint::Length(1)   Header bar
Constraint::Min(0)      Body ── split ──► Length(16) Sidebar │ Min(0) Canvas
Constraint::Length(3)   Player bar
```
Zen mode replaces the entire body and hides the sidebar. Minimum supported terminal is **80×24**;
below that, render a centred "terminal too small (needs 80×24)" message rather than a broken layout.

### Responsive degradation
| Width | Behaviour |
| :-- | :-- |
| ≥ 160 | 3 data columns + inspector; all player-bar fields |
| 120–159 | 3 columns; inspector narrowed to 24 cells |
| 100–119 | 2 columns + inspector |
| 80–99 | 1 column; inspector hidden; player bar drops bitrate and output |

## 3. Header bar

`loxia │ <server> │ <active tab> │ [OFFLINE] [↓3] [⇄] [⏱30m] │ <clock>`

Right-aligned badges appear only when active. A `?` hint sits at the far right when no modal is open.

## 4. Sidebar (`widgets/sidebar.rs`)

Nine tabs, each `<glyph> <name>` with the `Alt+N` number dimmed on the right. The active tab uses
the theme's `accent` background; the focused zone is marked `▶`. Registers `HitTarget::SidebarTab`.

## 5. Miller columns (`views/miller.rs`)

The core view. Details that matter:

- **Sliding window.** `nav.window_start` is recomputed on every drill and pop so the focused column
  is always the rightmost visible data column. Data columns share equal width; the inspector is
  fixed-width.
- **Parent path indicator.** When `window_start > 0`, the leftmost visible column's title becomes
  `…/<parent name>/`.
- **Section headers** (`── ALBUMS (2) ──`, `── APPEARS ON (2) ──`) are `MediaItem::SectionHeader`
  rows: dimmed, centred, fill-dashed, and **never selectable**. `MoveDown` from the row above a
  header lands on the row *below* it. This is the single most commonly-broken detail — there is a
  dedicated test.
- **Appears-On highlighting.** In a tracks column whose parent album is `AppearsOn`, tracks whose
  `artist_ids` contain the context artist use `accent`; the rest use `dim`.
- **Visual select.** A `[X]` / `[ ]` prefix column appears only while `selection.visual_mode`. The
  cursor highlight is independent of selection state.
- **Load states.** `Loading` renders a centred spinner; `Error` renders the message plus a retry
  hint drawn from the keymap; empty renders a themed empty-state line.
- Every visible row registers `HitTarget::ColumnItem`.

## 6. Inspector (`widgets/inspector.rs`)

Rightmost pane; content follows the focused column's selection.

| Selection | Contents |
| :-- | :-- |
| Artist | thumbnail, name, album/track counts, genres, wrapped scrollable overview |
| Album | art, title, artist, year, track count, total time, format summary, favourite and download state |
| Track | art, title, artists, album, year, disc/track numbers, duration, codec/rate/depth/bitrate, play count, applied ReplayGain, availability |
| Multi-select (n > 1) | `n Tracks Selected`, total time, and the contextual action list |

The action list at the bottom (`[a] Queue Selected`, `[Ctrl+P] Add to Playlist`, …) is generated
from the live keymap.

## 7. Player bar (`widgets/player_bar.rs`)

```
▶ <title> — <artist> (<album>)                                    [availability glyph]
<pos> [progress] <dur>
🎚 <codec> <depth>/<rate> │ Bitrate: <kbps> │ Vol: <pct>% │ EQ: <state>
```
- Line 3 collapses fields right-to-left as width shrinks.
- The progress bar registers `HitTarget::SeekBar` and supports click and drag seeking.
- Buffering uses a distinct fill glyph in the unplayed region.
- Sub-cell precision uses the partial block glyphs `▏▎▍▌▋▊▉█`; `theme.ascii_only` substitutes a
  plain `#`/`-` set.

## 8. Now Playing (`views/now_playing.rs`)

Two panes; the left toggles Queue ⇄ History with `H`.

- **Queue:** the current entry marked `▶`, upcoming numbered, played entries dimmed, each with a
  source badge (album / playlist / mix). Reorder with `Ctrl+↑`/`Ctrl+↓`, remove with `x`, jump with
  `Enter`.
- **History:** `• HH:MM  Artist — Title   duration`, newest first, capped at 50.
- **Right pane:** the now-playing block, a transport glyph row (`⏮ ⏯ ⏭ 🔀 🔁`, each a hit target),
  and the lyrics view.

### Lyrics (`widgets/lyrics.rs`)
Shown when `ui.show_lyrics` and the track has a `lyric_stream`. Synced lyrics mark the active line
with `▶` and auto-scroll to keep it vertically centred; the active index is derived from
`player.position` at render time, never stored. Unsynced lyrics render as a plain scrollable block.
A track with no lyrics hides the pane entirely rather than showing an empty box.

## 9. Other views

| View | Notes |
| :-- | :-- |
| `search.rs` | Query line focused on tab entry; three result sections (Artists / Albums / Tracks) with counts; **250 ms input debounce**; `Tab` cycles sections; drilling a result hands off to the Miller stack |
| `favourites.rs` | Same three-section layout from `Filters=IsFavorite`; `f` unfavourites in place with optimistic removal |
| `playlists.rs` | Miller: playlists → tracks. Reorder in place, `x` removes a track, `X` deletes the playlist behind a `Confirm` modal |
| `genres.rs` / `folders.rs` | Miller stacks with different `ColumnKind` seeds; folders show `📁`/`♪` glyphs |
| `settings.rs` | Sectioned form — Servers, Audio, Cache, Transcode, Interface, Sorting, Equalizer, Keybindings, About. Each row is a typed control (toggle / select / slider / text / action). Edits emit `Action::System::ConfigChanged`; persistence is debounced 1 s. Keybindings shows the conflict badge from `04` §7 |
| `zen.rs` | Centred album art on the left; title, album, and lyrics on the right; full-width progress and the format line below |

### Zen layout
```
┌─ ZEN FOCUS VIEW ──────────────────────────────────────────────────────────────┐
│                                                                               │
│    ┌──────────────────┐    Boy Harsher — Motion                               │
│    │                  │    Album: Care (2019)                                 │
│    │  [ Kitty/Sixel   │                                                       │
│    │    Album Art ]   │    LYRICS                                             │
│    │                  │    [01:12.10] I see you in the dark                   │
│    └──────────────────┘  ▶ [01:24.00] Moving through the emotion              │
│                            [01:31.50] Soft light, heavy sound                 │
│                                                                               │
│ 01:24 [████████████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░] 03:31        │
│ 🎚 FLAC 24-bit / 96.0 kHz │ Bitrate: 2840 kbps │ Vol: 100% │ EQ: Off        │
└───────────────────────────────────────────────────────────────────────────────┘
```

## 10. Modals

All modals are centred, `Clear` their region first, and carry a bordered title. `Esc` always closes;
`Enter` always submits the primary action. Each footer lists its keys, generated from the keymap.

The modal set is: `Help`, `Equalizer`, `DevicePicker`, `SleepTimer`, `SavePlaylist`, `SortProfile`,
`KeymapEditor`, `Confirm`. Layouts follow `design_overview` §§3.3–3.6 and 3.8 literally; those
blocks are the visual acceptance criteria.

**Equalizer:** bars drawn with vertical block glyphs from a 0 dB centre line. `←`/`→` selects a band
and highlights its column, `↑`/`↓` adjusts by ±0.5 dB, `p` cycles presets, `b` bypasses. Each band
registers `HitTarget::EqBand` for click and drag.

## 11. Album art (`widgets/album_art.rs`)

`ratatui-image` with protocol detection at startup: Kitty graphics → Sixel → iTerm2 → half-blocks →
off. `ui.album_art_protocol` forces a choice.

Decoding happens **off the main thread**: `Effect::Net(FetchImage)` → the worker decodes and resizes
→ `Event::ImageLoaded`. The render path only blits. Decoded images are held in an LRU of 16 keyed by
`(item_id, cell_size)` — re-encoding per frame would melt the terminal.

## 12. Themes (`assets/themes/*.toml`)

Seven themes: `default_terminal`, `far_blue`, `darcula`, `cyberpunk_neon`, `amber_crt`, `green_crt`,
`oled_black`. Each defines semantic roles, never raw colours at call sites:

`bg, fg, dim, accent, accent_alt, selection_bg, selection_fg, border, border_focus, header_bg,
success, warning, error, progress_filled, progress_empty`

`default_terminal` leaves `bg` unset (`Color::Reset`) so terminal transparency works — **do not
paint the background in that theme.** `oled_black` pins `bg = #000000`. Theme changes apply live
without a restart; a malformed theme file falls back to `default_terminal` with a warning.

## 13. Accessibility and robustness

- **Never rely on colour alone.** Selection carries `▶`, favourites `♥`, downloads `↓`, unavailable
  `⚠`.
- Every glyph has an ASCII fallback, selected by `ui.ascii_only` or the `LOXIA_ASCII=1` env var.
- `Resize` recomputes layout only; cursor and scroll state are preserved.
- All text is truncated at **grapheme** boundaries (`unicode-segmentation`) and measured with
  `unicode-width` (`text.rs`). Without this, CJK and emoji titles corrupt column borders.

## 14. Snapshot testing

`insta` with `ratatui::backend::TestBackend` at 80×24, 120×30, and 200×50, using fixed fixture state
and a fixed clock.

Required snapshots: miller-3-columns, appears-on-split, visual-select, now-playing-queue,
now-playing-history, zen-with-lyrics, zen-without-lyrics, each modal, offline banner, empty library,
column error state, and one Miller view per theme to catch unreadable colour pairings.
