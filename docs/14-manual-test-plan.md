# 14 — Manual Test Plan

A full sweep of every user-facing feature, for use as a regression checklist before a release or
after a batch of changes. It is deliberately exhaustive rather than risk-ranked: §22 is the part
that catches *known* regressions, but a bug that has never happened before will only be caught by
walking the rest.

Automated tests cover the reducer, the parsers and the widgets in isolation. What they cannot cover
is the part where loxia meets a real terminal, a real Emby server and a real audio device — every
defect in §22 slipped through a green test suite. Assume nothing here is covered elsewhere.

## How to use this

Work top to bottom in one sitting per platform where possible; several sections depend on state
built by earlier ones (a queue exists, a track has played, a download is present).

For each item: **✅** works, **❌** broken, **➖** not applicable to this environment. For anything
not ✅, record what you saw, what you expected, the terminal, and the relevant lines from
`~/.local/state/loxia-player/logs/` (or the platform equivalent). A screenshot of a layout defect is worth
more than a description.

Record the environment for the run:

| | |
| :-- | :-- |
| loxia version / commit | |
| OS and version | |
| Terminal emulator and version | |
| mpv / libmpv version | |
| Emby server version | |
| Library size (artists / albums / tracks) | |
| Date | |

---

## 1. First run and connection

- [✅] Launch with **no config file** — starts, does not crash, lands somewhere usable.
- [✅] A config file is created at the documented path with sane defaults.
- [✅] Add a server: URL, username, password. Connects and reports success.
- [✅] Add a server with a **wrong password** — a clear error, the app stays usable, no crash.
- [✅] Add a server with an **unreachable URL** — same: error, still usable.
- [✅] A server with **custom headers** configured connects and passes them.
- [✅] Restart — reconnects automatically without re-entering credentials.
- [✅] The access token is **not** visible in logs or in the About view's diagnostics.
- [✅] With several music libraries, browsing lists span **all** of them (see §22.14).

## 2. Sidebar and tab navigation

Ten tabs in order: Now Playing, Favourites, Search, Playlists, Artists, **Album Artists**, Albums,
Genres, Folders, Settings — plus a Quit row beneath them.

- [✅] `Tab` / `Shift+Tab` cycle forward and backward through all ten.
- [✅] `Alt+1`…`Alt+9` jump to tabs 1–9; **`Alt+0` jumps to Settings** (the tenth).
- [✅] `F2`…`F9` mirror `Alt+2`…`Alt+9`; `F10` mirrors `Alt+0`.
- [✅] **`F1` opens Help**, not tab 1 (§22.8).
- [✅] `↑`/`↓` (and `k`/`j`) move between tabs when focus is on the sidebar.
- [✅] `↓` past the last tab reaches the **Quit** row; `↑` comes back.
- [✅] `Enter` or `→` on the Quit row exits cleanly.
- [✅] Clicking a tab with the mouse switches to it.
- [✅] Clicking the Quit row exits.
- [✅] Each tab shows its shortcut digit and glyph; `ascii_only` themes show ASCII substitutes.

## 3. Miller-column browsing (Artists, Album Artists, Albums, Genres, Folders)

- [✅] Each tab loads its root column on first visit; a spinner shows while loading.
- [✅] `j`/`k`/`↑`/`↓` move the cursor; `l`/`→`/`Enter` drills in; `h`/`←`/`Backspace` steps out.
- [✅] `g g` / `Home` jumps to the top, `G` / `End` to the bottom.
- [✅] `Ctrl+U`/`PageUp` and `Ctrl+D`/`PageDown` move by half a page.
- [✅] At most three columns are visible; the focused column is always the rightmost.
- [✅] **Scrolling past the first page loads more** — by keyboard *and* by mouse wheel (§22.13).
- [✅] Scroll the wheel over a column you have **not** focused — that column scrolls, focus does not move.
- [✅] `/` opens the inline filter; typing narrows the list; `Esc` cancels; `Enter` commits.
- [✅] A filter finds an item that is **past the first page** (it keeps pulling pages).
- [✅] Artists shows a **track count** per artist, never `0 albums` (§22.6).
- [✅] Album Artists is a **shorter list** than Artists on a library with compilations.
- [✅] Albums shows the year; Tracks shows durations.
- [✅] An artist with compilations shows the **ALBUMS / APPEARS ON** split with section headers.
- [✅] Section headers cannot be landed on by the cursor.
- [✅] Genres: drilling a genre lists its artists.
- [✅] Folders: the root lists **every** music library; drilling reaches real directories and files.
- [✅] An empty column shows a "nothing here — …" message naming the level.
- [ ] `Ctrl+R` retries a column that failed to load; the error text and retry hint are visible.

## 4. Inspector (right-hand metadata pane)

- [✅] Artwork appears **above** the metadata for an artist, an album artist, and an album (§22.16).
- [✅] Selecting a track shows codec, bitrate, duration, and ReplayGain information.
- [✅] Selecting several items (visual mode) shows a count and total duration.
- [✅] The action legend lists the applicable actions with their **current** key bindings.
- [✅] On an **Appears On** album the legend names both choices explicitly (§22.17).
- [✅] Clicking an action row in the legend performs it.
- [✅] Settings → Interface → **inspector artwork = off** reclaims the rows and fetches nothing.

## 5. Queueing semantics

- [✅] `Enter` on a track **replaces** the queue and plays immediately.
- [✅] `A` **appends** to the queue and reports how many tracks were added (§22.20).
- [✅] `Enter` on an album plays it from the **first** track, not the second.
- [✅] `Enter` on an artist plays their tracks without needing the albums loaded first.
- [✅] `Enter` on a folder queues its **entire subtree**, in filename order.
- [✅] `Enter` on an **Appears On** album queues only the selected artist's tracks.
- [✅] `A` on that same album queues the **whole** compilation.
- [✅] `Enter` on a genre queues **every** track in it, and the toast reports the count.
- [✅] A queued genre is ordered by the **configured sort profile**, with albums kept whole —
      changing the profile in Settings changes the resulting queue order.
- [✅] A queued artist is ordered the same way; albums are not interleaved by track number.
- [✅] A queued **album** still plays in disc/track order regardless of the sort profile.
- [✅] `i` inserts the selection immediately after the current track.
- [✅] `i` on an **album** or **artist** row inserts all of its tracks after the current one, not at
      the end of the queue (§22.22).
- [✅] `m` starts an instant mix seeded from the selection.
- [✅] `v` enters visual select, `.` toggles an item, `V` selects all, `Esc` clears.
- [ ] `Esc` clears the selection **and** the count in one press; leaving the tab clears it too
      (§22.28).
- [ ] With several **albums** selected, the inspector says "N Albums Selected", not "N Tracks
      Selected", and does not offer Add to Playlist / Download (§22.24).
- [✅] Queueing a multi-selection preserves the displayed order.
- [✅] `v`-selecting several **albums** and pressing `a` queues all of them, in the order the rows
      are displayed — not in the order the server happens to answer (§22.22).
- [✅] A selection holds **one kind of row at a time**: with folders selected, `.` on a track is
      refused with a toast, and `V` selects only the folders.

## 6. Transport and playback

- [✅] `Space` plays and pauses.
- [✅] `n` / `p` skip to next and previous.
- [✅] `S` stops (§22.4).
- [✅] `]` seeks **forward 5 seconds** — not to the end of the track (§22.10).
- [✅] `[` seeks back 5s; `}` forward 30s; `{` back 30s.
- [✅] Seeking near the start and near the end clamps sensibly rather than skipping tracks.
- [✅] `+`/`=` and `-`/`_` change the volume, and the change is **audible** (§22.10).
- [✅] The volume readout in the status bar tracks the change.
- [✅] `M` mutes and unmutes; the readout shows `muted`.
- [✅] Playback advances **gaplessly** from one track to the next across a whole album.
- [✅] The track advances automatically at the end of a track.
- [ ] Every track in a long queue actually plays — none are silently skipped.
- [ ] Play counts increment in the Emby web UI.
- [ ] A long **pause** (10+ minutes) still resumes correctly (§22.3).

## 7. Queue management

- [✅] `s` shuffles; the current track keeps playing; `s` again restores the original order.
- [✅] `R` cycles repeat Off → All → One; the header badge reflects it.
- [ ] Repeat One replays the same track; Repeat All wraps at the end.
- [✅] `o` opens the sort menu; applying a profile reorders the queue.
- [✅] The menu's first row is **Default order (as queued)**; applying it restores the order the
      tracks were added in (album disc/track, playlist order, folder paths) and clears the profile.
- [✅] Sorting, then restoring the default, then sorting again all keep the current track playing.
- [✅] Sorting works while **paused** as well as while playing (§22.5).
- [ ] An `artist + year + album + track` profile groups albums correctly — the year matches the
      album, not individual tracks (§22.5).
- [✅] `x` removes the entry under the cursor; removing the **current** track advances playback.
- [✅] `Ctrl+Up` / `Ctrl+Down` reorder a track within a playlist.
- [✅] Replacing the queue while paused **starts playing** rather than staying paused (§22.5).

## 8. Now Playing tab

- [✅] The queue list shows position, title, artist and duration — **no** per-row source badge.
- [✅] After `m`, the pane is titled "MIX FOR <name>"; queueing anything else returns it to
      "PLAY QUEUE" (§22.29).
- [✅] Clicking a queue row moves the cursor **without scrolling the list**, so a double-click on a
      row plays that row (§22.29).
- [✅] The currently playing entry is marked and highlighted.
- [✅] `j`/`k` move the cursor; the pane auto-follows the track change until you scroll manually.
- [✅] **The mouse wheel scrolls the queue** (§22.12).
- [✅] The wheel also scrolls over the History sub-view and over blank space below a short queue.
- [✅] A single click moves the cursor; a **double click** jumps playback to that entry.
- [✅] `H` toggles History; `H` again returns to the queue.
- [✅] `Enter` on a history row re-queues that track.
- [✅] An empty queue shows a hint naming the key to press.

## 9. Zen mode

- [✅] `z` enters and leaves Zen.
- [] Artwork, title, artist and album are shown, centred.
- [✅] **The status bar does not change size or shift** when toggling Zen (§22.9).
- [✅] The seek bar and technical readout stay on the same screen rows either side of the toggle.
- [✅] There is exactly **one** status bar, not two (§22.2).
- [ ] Zen is two panes: artwork + track/artist/album on the **left**, lyrics on the **right**, with
      **no** border or rule between them (§22.32).
- [ ] Toggling lyrics inside Zen does **not** move the artwork or the title block (§22.7).
- [ ] A track **without** lyrics centres the artwork and title across the full width.
- [✅] Transport buttons in the Zen footer work with the mouse.
- [✅] Zen at 80×24 and at a very large window size both render sensibly.

## 10. Lyrics

- [✅] `L` shows and hides the lyrics pane, in Now Playing and in Zen.
- [✅] A track with an `.lrc` sidecar shows **synced** lyrics that scroll and highlight in time.
- [✅] A track with only a plain-text transcript shows readable lyrics.
- [✅] **No timecodes or cue numbers appear as lyric text** (§22.19).
- [✅] A track with *both* a transcript and a timed sidecar uses the **timed** one (§22.19).
- [✅] `K` / `J` scroll untimed lyrics up and down on a long track.
- [✅] Scrolling clamps at both ends and resets when the track changes.
- [✅] A track with no lyrics hides the pane entirely rather than showing an empty box.
- [✅] Lyrics still load after a **session restore**, not only after a manual track change (§22.7).
- [✅] Toggling lyrics does not move the artwork in either view (§22.7).

## 11. Artwork

- [✅] Album art appears in Now Playing, in Zen, and in the inspector.
- [✅] **Album art appears for albums**, not only after drilling into their tracks (§22.18).
- [✅] Art is sized to the pane rather than a small blocky thumbnail (§22.7).
- [ ] A cover **smaller** than the space reserved for it sits centred in that space — in Now
      Playing and in Zen — not pushed to one side (§22.42).
- [✅] Art does not jump or resize when unrelated things change.
- [✅] In a graphics-capable terminal (Kitty, Ghostty, WezTerm, Konsole) real images render.
- [ ] In a terminal **without** graphics support, a placeholder renders and nothing crashes.
- [ ] Settings → Interface → album art protocol: each option behaves or degrades gracefully.
- [✅] A track with no artwork anywhere shows the placeholder.

## 12. Player bar / status bar

- [✅] Line 1 shows the now-playing text per `ui.now_playing_format`; editing that template works.
- [✅] Line 2 shows elapsed time, a seek bar, total time, and the transport buttons.
- [✅] Line 3 shows codec, bit depth / sample rate, bitrate, source, volume, and EQ state.
- [✅] **All six transport buttons respond to a click**: prev, play/pause, stop, next, shuffle,
      repeat — reliably, on the first click, every time (§22.11).
- [✅] Clicking the seek bar seeks to that position; dragging scrubs.
- [✅] The **bitrate does not shift the fields beside it** as it fluctuates (§22.6).
- [✅] The volume readout sits immediately before the EQ segment.
- [ ] `Source: cache` / `Source: stream` reflects where the track is actually playing from.
- [✅] Narrowing the terminal drops the lowest-priority segments and never wraps or overflows.
- [ ] The header shows server, tab, badges (offline, downloads, shuffle, repeat, timer, warnings)
      and the clock.
- [ ] The header's **`[?]` button** opens Help, and sits left of the clock.

## 13. Search, Favourites, Playlists

- [ ] `/` on the Search tab focuses the query line; typing searches after a short debounce.
- [ ] Results appear in Artists / Albums / Tracks sections with counts.
- [ ] `↓` from the query line moves into the results; arrows move within them.
- [ ] `Tab` cycles between result sections.
- [ ] `Enter` on an artist or album result drills into it **and `←` returns to a browsable list**.
- [ ] `Enter` on a track result plays it.
- [ ] `f` toggles favourite on a track, album and artist; the Favourites tab reflects it
      **without** needing `Ctrl+R` (§22.43).
- [ ] `f` works in **Now Playing** (on the row under the cursor) and in **Zen** (on the playing
      track), and both show a `♡` for a favourited track (§22.43).
- [ ] `f` on a **playlist** row favourites it, and it appears in a `PLAYLISTS` section of the
      Favourites tab, playable from there with `Enter` (§22.44).
- [ ] Favourites shows its three sections and plays from them.
- [ ] Arrows move the cursor **through** the favourites — across all three sections — rather than
      switching sidebar tabs (§22.23).
- [ ] `←` from the favourites parks focus on the tab sidebar; `→` steps back in.
- [ ] `Enter`, `A`, `i`, `m` and `f` all work on a favourited track, album and artist (§22.23).
- [ ] Clicking a row in Search or Favourites moves the cursor to it; double-clicking plays it.
- [ ] `P` saves the queue as a new playlist; it appears in Emby.
- [ ] `Ctrl+P` adds the selection to an existing playlist.
- [ ] `X` deletes a playlist, **after a confirmation prompt**.
- [ ] The Playlists tab lists **exactly** the playlists the official Emby client shows — no
      folders, albums or individual tracks (§22.1).
- [ ] Opening a video playlist shows nothing playable rather than failing (§22.1).
- [ ] Playlists are not **duplicated** in the list (§22.1).

## 14. Audio pipeline

- [ ] `e` opens the equalizer; adjusting a band changes the sound immediately.
- [ ] **`Enter` keeps the edit** — the sound stays changed after the modal closes — while `Esc`
      discards it, and the footer names both (§22.31).
- [ ] `t` turns the equalizer **off** from the modal; the title reads `Status: OFF` and the sound
      returns to flat. It must **not** be `o` — that opens the sort menu everywhere else (§22.31).
- [ ] Presets apply; `b` bypasses without uninstalling; submitting while bypassed stays flat.
- [ ] EQ settings — including off — survive a restart (§22.5).
- [ ] `r` cycles ReplayGain Album → Track → Off, with a toast, applied to the current track.
- [ ] `O` opens the device picker, grouped by driver; switching device works **mid-track** and
      playback continues from the same position.
- [ ] Switching to an invalid device rolls back rather than leaving silence.
- [ ] `q` cycles quality profiles and the stream actually changes.
- [ ] Changing quality in **Settings** has the same effect as `q` (§22.15).
- [ ] `T` opens the sleep timer: a duration, end-of-track and end-of-queue variants.
- [ ] The sleep timer fades out and stops; the header badge counts down.
- [ ] Cancelling a sleep timer restores the pre-fade volume.

## 15. Cache, downloads and offline

- [ ] Settings → Cache → enabled: playing tracks populates the cache.
- [ ] Cached files are laid out as `tracks/<server>/<AlbumArtist>/<Album>/…` (§22.21).
- [ ] `cache_index.json` exists in the cache root after a session, and the cache is **reused** on
      the next launch instead of re-downloading (§22.34).
- [ ] Add a second profile for the **same** server on a different address: it plays from the cache
      the first one filled, and restores the same saved session (§22.38).
- [ ] Add `[[servers.fallbacks]]` with a second address: stopping the primary mid-session
      reconnects on the fallback, with a toast, and About shows `Connected … (fallback)` (§22.39).
- [ ] In **Settings → Servers**, the `address:` row selects primary/fallback with `[←→]`,
      `[ add another address ]` adds one and `x` removes it; each address keeps its own headers and
      can be tested on its own (§22.39).
- [ ] Saving **while a fallback is selected** leaves the primary's own address untouched; each
      address keeps its own values through switching, saving and reopening (§22.39).
- [ ] Cached filenames carry the **correct extension** for their codec — an MP3 is not `.flac`
      (§22.21).
- [ ] `prefetch next tracks = 2`: upcoming tracks appear in the cache while the current one plays.
- [ ] Read-ahead **never interrupts playback**.
- [ ] `prefetch on play = on`: the playing track lands in the cache, and playback is unaffected.
- [ ] Playing a cached track shows `Source: cache`; a fresh one shows `Source: stream`.
- [ ] `d` pins a download; the header download badge appears; the file lands in the downloads tree.
- [ ] `d` works on an **album**, an **artist** and a **playlist** row, not only a track (§22.37).
- [ ] `d` over a visual multi-selection downloads every selected row (§22.37).
- [ ] A pinned row shows `↓` in the list, and `d` again removes the download (§22.37).
- [ ] A downloaded track plays from the downloads tree, not the network — including while a
      transcode profile is active — and does not appear again in the rolling cache (§22.40).
- [ ] Disconnect the network: downloaded content is browsable and playable offline.
- [ ] The OFFLINE badge appears; actions needing a connection refuse with a toast rather than hanging.
- [ ] Reconnect: buffered play counts and scrobbles are sent.
- [ ] Playing a track through raises its Emby **`PlayCount`** and sets `LastPlayedDate` — a
      server-side Last.fm plugin scrobbles from exactly that (§22.25).
- [ ] Pausing shows the track as **paused** in Emby's dashboard, with the position frozen; resuming
      un-pauses it. A long pause must never accumulate play time or extra scrobbles (§22.26).
- [ ] Pause works **while the track is buffering**, not only while it is smoothly playing (§22.26).
- [ ] Exceeding the rolling cache size evicts the least recently used entries, never the playing
      track and never a download.
- [ ] Restart after filling the cache: startup reconciliation logs sensible numbers and deletes
      nothing it should keep.
- [ ] `.part` files do not accumulate.

## 16. Settings

For each section — Servers, Audio, Cache, Transcode, Interface, Sorting, Equalizer, Keybindings,
About:

- [ ] The section list navigates with the keyboard and by clicking.
- [ ] `Esc` from a control list returns to the section list; `Esc` again to the tab sidebar (§22.2).
- [ ] Every control edits: toggles flip, sliders move, numbers accept input, selects cycle,
      text fields edit and commit.
- [ ] An invalid value is rejected with an inline error rather than being stored.
- [ ] Changes persist across a restart.
- [ ] **Theme** applies immediately, and the background is themed — not just the text (§22.5).
- [ ] Theme survives a restart (§22.5).
- [ ] Every one of the seven built-in themes renders legibly.
- [ ] `ascii_only` replaces spinners, bars, borders and glyphs.
- [ ] Mouse toggle takes effect immediately.
- [ ] Sorting: create, edit, reorder and delete profiles; **delete asks for confirmation** (§22.5).
- [ ] Adding a new sort profile works from an empty list (§22.5).
- [ ] "Restore default sort profiles" repopulates them (§22.5).
- [ ] Servers: add, edit, remove; removing offers to delete that server's cached data; the active
      server cannot be removed.
- [ ] About shows versions and cache/download statistics; "copy diagnostics" redacts the token.
- [ ] "Clear saved session" empties the queue and the next launch starts clean.

## 17. Keybindings and help

- [ ] `?` and `F1` both open the help overlay; it lists **every** action grouped by category.
- [ ] Help scrolls; `Esc` closes it.
- [ ] The help overlay shows the **current** binding for each action, not a hardcoded one.
- [ ] Settings → Keybindings lists every action and its binding.
- [ ] Rebinding an action works and takes effect immediately.
- [ ] Attempting to bind an already-used chord is refused, naming the incumbent action.
- [ ] "Reset all bindings" asks for confirmation and restores the defaults.
- [ ] A deliberate conflict written into `config.toml` produces a startup toast, a Settings badge,
      and `WARN` log lines — and the app still starts.
- [ ] Walk the **entire** default binding table and confirm each key does what it claims:
      `j k h l ↑ ↓ ← → Ctrl+U Ctrl+D PageUp PageDown gg Home G End Backspace Tab Shift+Tab
      Alt+1..9 Alt+0 F2..F10 / g a g l Esc Space n p S [ ] { } + = - _ M Enter a A i m s R o x v .
      V e r q O T f d P Ctrl+P X Ctrl+Up Ctrl+Down z H L K J ? F1 Ctrl+Q Ctrl+C Ctrl+R`

## 18. Session and lifecycle

- [ ] Quit with `Ctrl+Q`; relaunch — queue, position, current track, tab and volume are restored,
      and playback is **paused**.
- [ ] `restore_autoplay = true` resumes playing instead.
- [ ] Pressing `Space` on a restored-but-unloaded session starts playback from the saved position.
- [ ] Selecting something new on a restored session plays it rather than silently appending (§22.3).
- [ ] `Ctrl+C` quits as cleanly as `Ctrl+Q`.
- [ ] Killing the terminal leaves no orphan process and no corrupt index files.
- [ ] The terminal is restored on exit — no raw mode, no alternate screen left behind.

## 19. Integrations

- [ ] OS media keys (play/pause, next, previous) control playback.
- [ ] The OS now-playing panel shows title, artist, album and artwork.
- [ ] Seeking and position updates are reflected there.
- [ ] Desktop notifications appear on track change; disabling them in Settings stops them.
- [ ] WebSocket remote control: play/pause/next/previous/seek/volume/mute from the Emby web UI or
      another client are obeyed.
- [ ] A library change on the server refreshes the affected view.
- [ ] Disabling `enable_websocket` stops remote control and the app still works.

## 20. Resilience and edge cases

- [ ] Resize to exactly 80×24 — the minimum — everything is usable.
- [ ] Resize below the minimum — a clear "terminal too small" message, no panic.
- [ ] Resize continuously **during playback** — no crash, no corrupt frame.
- [ ] A very wide/tall window renders without artefacts.
- [ ] Unicode-heavy metadata (CJK, Cyrillic, emoji, RTL) renders without breaking column widths.
- [ ] Track/album names containing `/`, `\`, `:` and other path characters cache and download safely.
- [ ] Server goes away mid-playback — a clear state, no hang, recovery on return.
- [ ] Rapidly skipping through many tracks does not queue up runaway downloads or crash.
- [ ] Leave it playing for an hour — no drift, no leak, no degradation.
- [ ] Check the log file: no unexpected `WARN`/`ERROR`, no `unhandled effect` lines.

---

## 21. Platform-specific

- [ ] **Linux**: build from a clean checkout; mpv from the package manager; MPRIS visible in the DE.
- [ ] **macOS**: mpv via Homebrew; the now-playing centre populated; media keys work.
- [ ] **Windows**: no pre-installed mpv; the bundled/instructed libmpv path works; SMTC populated.
- [ ] Fresh install on a clean machine or VM for each platform.
- [ ] Uninstall leaves only documented user data behind.

---

## 22. Regression sweep — defects previously found in the field

Every item below was a real bug reported from live use. Each is the cheapest possible check that it
has not returned. **This is the highest-value section of this document.**

1. **Playlists doubled / wrong / video playlists offered.** The Playlists tab lists each playlist
   once, lists exactly what the official client lists (a `MediaTypes=Audio` filter once made a real
   server answer with 106,115 folders, albums and tracks in place of 3 playlists), and a video
   playlist opens empty instead of failing.
2. **Settings dead ends.** `Esc` from a settings control returns to the section list and then to
   the sidebar. Modals do not show the view beneath them through their body.
3. **Restore-then-play.** After a restart, selecting something new plays it immediately. A pause
   longer than the server's stream timeout still resumes.
4. **Stop / Quit.** `S` stops playback. The sidebar Quit row is reachable by keyboard *and* mouse.
5. **Settings that did nothing.** Theme (including the background), equalizer, transcode quality
   and ReplayGain all take effect immediately *and* survive a restart. Sort profiles can be added
   from empty, deleted only after confirmation, and restored to defaults. Sorting works while
   paused. Replacing the queue while paused starts playing.
6. **Player-bar instability.** The bitrate readout does not shove its neighbours sideways as it
   fluctuates. The Artists column shows a real track count, never `0 albums`.
7. **Lyrics and artwork layout.** Lyrics load after a session restore. Toggling the lyrics pane
   does not move the artwork or the title block, in Now Playing or in Zen. Artwork fills its pane
   rather than rendering as a small blocky thumbnail.
8. **`F1`.** Opens Help. It does not jump to the first tab.
9. **Zen footer.** The bottom panel does not change height or shift its contents when Zen is
   toggled.
10. **Seek and volume.** `]` seeks five seconds forward, not to the end of the track. `+`/`-`
    audibly change the volume.
11. **Transport buttons.** Every button in the status bar responds to a click, first time — in
    particular **Stop does not skip a track**, and **Play works right after launch**.
12. **Now Playing wheel.** The mouse wheel scrolls the queue.
13. **Miller pagination.** A long list keeps loading past the first page when scrolled with the
    **mouse wheel**, not only with the keyboard.
14. **Multi-library.** With more than one music library, Artists, Album Artists, Albums and Genres
    span all of them.
15. **Settings vs keybinding parity.** Changing transcode quality in Settings does what `q` does.
16. **Inspector artwork.** Artist and album artwork appears above the metadata, and the setting
    that disables it reclaims the space.
17. **Appears-On queueing.** The inspector names both choices, and both work under **different,
    pressable keys** — `Enter` for this artist's tracks only, `A` for the whole album.
18. **Album artwork.** Albums show artwork in the inspector, not only their tracks.
19. **Lyrics content.** No timecodes or cue numbers are ever shown as lyric text; a track carrying
    both a transcript and a timed sidecar uses the timed one.
20. **Silent appends.** Adding to a playing queue reports what it added.
21. **Cache on disk.** Files are filed under album artist and album, and named with the extension
    matching their real codec.
22. **Container queueing.** `i` on an album or artist inserts its tracks after the current one.
    `v`-selecting several albums and pressing `a` queues all of them, in displayed order.
23. **Favourites is usable.** Arrows move a cursor through the three sections instead of switching
    tabs, and `Enter`/`A`/`i`/`m`/`f` and the mouse all act on the focused favourite.
24. **The multi-select panel tells the truth.** It names what is actually selected (Albums,
    Artists, Tracks) and offers only actions that apply to it.
25. **Plays are recorded server-side.** After playing a track to the end, its Emby `PlayCount`
    increases and `LastPlayedDate` is set — not merely "played". This is what server-side
    scrobbling plugins (Last.fm) hook, and it needs the `Sessions/Playing` report to go out first.
26. **Pause really pauses.** A press lands even during a rebuffer, and Emby is told about it:
    paused sessions report `IsPaused: true` with a frozen position, so a long pause never turns
    into extra scrobbles.
27. **Chrome glyphs sit straight.** No sidebar icon, row marker or player-bar glyph pushes the
    text beside it a column right — none of them is emoji-capable, so no terminal can draw one two
    cells wide.
28. **Selections do not outlive themselves.** `Esc` clears the selection along with the mode, and
    leaving a tab clears it too; the inspector never reports a count you cannot see.
29. **The queue pane holds still.** Clicking a row does not scroll the list, so a double-click
    lands on the row you aimed at.
30. **Emptying the queue stops the audio.** Removing the last entry actually stops playback rather
    than only saying it did.
31. **The equalizer applies and can be switched off.** `Enter` keeps an edit (`Esc` discards it,
    and the footer says so), and `t` turns the equalizer off from the modal itself.
32. **Zen is two panes, undivided.** Now-playing on the left, lyrics on the right, no rule between
    them; the left block never moves when lyrics are toggled.
33. **Every modal is on the themed surface.** No gaps between text showing the terminal's own
    background — the help sheet is the one to look at.
34. **The cache survives a restart.** `cache_index.json` exists after a session, and a track played
    again next launch loads from `file://` rather than re-downloading.
35. **The source readout tracks reality.** `stream` → `caching` → `cache` as a background fetch
    runs and finishes; the About page's cache size is not stuck at 0.
36. **Transcoded tracks show their real length.** The seek bar spans the whole track, not the part
    transcoded so far.
37. **`d` downloads.** It works on a track, album, artist and playlist, and on every row of a
    visual multi-selection; a pinned row shows `↓`; pressing it again removes the download.
38. **One server, one cache.** Two profiles pointing at the same Emby (a LAN address and an
    external one) share the cache, the downloads and the saved session instead of duplicating
    them; the cache tree is named after the server's own id, not the profile's.
39. **A profile survives losing an address.** With `[[servers.fallbacks]]` configured, an
    unreachable primary connects on the fallback at startup *and* switches over mid-session; an
    address answering as a different server is refused.
40. **A download is a cache.** A downloaded track plays from disk — under any quality profile —
    and is never re-fetched or re-transcoded.
41. **One listen, one scrobble.** A completed track produces a single play on the server: exactly
    one `Sessions/Playing` … `Stopped` pair and no separate mark-as-played.
42. **Artwork is centred in its box.** A cover that does not exactly fill the space reserved for
    it is centred there, in both play views.
43. **Favouriting works everywhere and is visible.** `f` works in Now Playing and Zen as well as
    the browsing lists, each marks the track, and the Favourites tab picks the change up on the
    next visit rather than staying stale until `Ctrl+R`.
44. **Playlists can be favourited.** `f` works on a playlist row, and the Favourites tab has a
    `PLAYLISTS` section; the Search tab does not grow an empty one.

---

## 23. Sign-off

| Section | Result | Notes |
| :-- | :-- | :-- |
| 1 First run and connection | | |
| 2 Sidebar and tabs | | |
| 3 Miller browsing | | |
| 4 Inspector | | |
| 5 Queueing semantics | | |
| 6 Transport and playback | | |
| 7 Queue management | | |
| 8 Now Playing | | |
| 9 Zen | | |
| 10 Lyrics | | |
| 11 Artwork | | |
| 12 Player bar | | |
| 13 Search / Favourites / Playlists | | |
| 14 Audio pipeline | | |
| 15 Cache and offline | | |
| 16 Settings | | |
| 17 Keybindings and help | | |
| 18 Session and lifecycle | | |
| 19 Integrations | | |
| 20 Resilience | | |
| 21 Platform-specific | | |
| 22 Regression sweep | | |
