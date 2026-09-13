# 09 — Traceability

Every requirement in `design_overview` mapped to the task that implements it. Use this to check
coverage, and to find where a feature lives when a bug report arrives.

The task library is `../tasks/`, 109 tasks across 13 phases.

## Navigation and layout (`design_overview` §2)

| Requirement | Tasks |
| :-- | :-- |
| Three-zone layout: header, sidebar, canvas, player bar | `04-02`, `04-05`, `04-09` |
| Ten sidebar tabs with `Alt+1`–`Alt+9`/`Alt+0` jumps (plus `F2`–`F10`) | `03-05`, `04-05` |
| Miller columns, max 3 visible | `04-06`, `04-07` |
| Sliding window with parent path indicator | `03-06`, `04-07` |
| Metadata inspector pane | `04-08` |
| ALBUMS / APPEARS ON split | `02-06`, `04-10` |
| `Enter`/`a` queues artist-only on compilations | `06-02` |
| `Shift+Enter`/`A` queues full context | `06-02` |
| Appears-on track highlighting | `04-06` |

## Wireframes (`design_overview` §3)

| Wireframe | Tasks |
| :-- | :-- |
| §3.1 Miller columns with visual multi-select | `04-06`, `04-07`, `04-11` |
| §3.2 Now Playing with History sub-view | `07-06` |
| §3.3 Help cheat sheet overlay | `10-03` |
| §3.4 Save queue to playlist | `10-08` |
| §3.5 Audio device quick-picker | `10-05` |
| §3.6 Sleep and shutdown timer | `09-05`, `10-07` |
| §3.7 Zen focus mode | `10-02` |
| §3.8 10-band equalizer overlay | `09-03`, `10-06` |

## Playback, audio and cache (`design_overview` §4.1)

| Requirement | Tasks |
| :-- | :-- |
| Direct FLAC streaming and transcode profiles (`q`) | `02-09`, `09-06` |
| Rolling LRU cache with eviction | `08-02`, `08-03` |
| Permanent offline downloads (`d`) | `08-04` |
| 0 ms gapless transitions | `06-06` |
| Bit-perfect hardware mode (`Ctrl+B`) | `09-02` |
| Audio device quick-picker (`O`) | `09-01`, `10-05` |
| 10-band equalizer (`e`) | `09-03`, `10-06` |
| ReplayGain album/track/off (`r`) | `09-04` |
| ~~Crossfade 0–10 s~~ | **cut** — `12-decisions.md` §3 |

## Queue, selection, history, playlists (`design_overview` §4.2)

| Requirement | Tasks |
| :-- | :-- |
| Session state persistence | `08-08`, `11-06` |
| Visual multi-select (`v`, `.`) | `03-06`, `04-06` |
| Listening history, last 50 (`H`) | `06-05`, `07-06` |
| Favourites tab | `07-02` |
| Save queue or selection to playlist | `07-03`, `10-08` |
| Non-destructive shuffle (`s`) | `06-03` |
| Multi-criteria sort profiles (`o`) | `06-04`, `10-09`, `11-04` |
| Instant mix (`m`) | `06-08` |

## Discovery, interface, aesthetics (`design_overview` §4.3)

| Requirement | Tasks |
| :-- | :-- |
| Universal help modal (`?`) | `10-03` |
| Full mouse support | `04-04`, `10-04` |
| Desktop toast notifications | `10-10` |
| Direct tab jumps | `03-05`, `04-05` |
| Zen focus mode (`z`) | `10-02` |
| Album art (Kitty / Sixel) | `10-01` |
| Synced lyrics | `01-08`, `02-11`, `07-07` |
| 7 visual themes | `04-01` |
| ~~Spectrum analyzer~~ | **cut** — `12-decisions.md` §3 |

## System, network, offline (`design_overview` §4.4)

| Requirement | Tasks |
| :-- | :-- |
| Offline scrobble sync buffer | `08-07` |
| Multi-server profiles | `11-03` |
| Arbitrary custom HTTP headers | `02-02`, `05-05`, `11-03` |
| In-UI keybinding remapper | `11-02` |
| Cross-platform media keys | `10-11` |
| Offline browsing and playback | `08-05`, `08-06` |
| WebSocket remote control | `10-12` |

## Distribution (`design_overview` §5)

| Requirement | Tasks |
| :-- | :-- |
| cargo-dist release matrix | `12-01` |
| Windows MSI with bundled `mpv-1.dll`, portable ZIP | `12-02` |
| Scoop and WinGet manifests | `12-02` |
| macOS DMG, PKG, Homebrew tap | `12-03` |
| Linux tarball, AUR, Homebrew | `12-04` |

## Keybindings (`design_overview` §6)

The original matrix contained 14 conflicts and no quit key. Replaced by the flat table in
`04-state-and-input.md` §6 — see `12-decisions.md` §4.

| Requirement | Tasks |
| :-- | :-- |
| Chord model and binding parser | `03-04` |
| Default keymap, conflict-free | `03-05` |
| Conflict detection and user notification | `03-05`, `10-03`, `11-01`, `12-08` |
| In-UI remapping | `11-02` |
| ~~`1`–`5` star ratings~~ | **cut** — Emby has no numeric rating, `12-decisions.md` §2 |

## Configuration (`design_overview` §7)

| Requirement | Tasks |
| :-- | :-- |
| `config.toml` schema | `01-01` |
| Defaults, validation, migration | `01-02` |
| Atomic writes, `0600` permissions | `01-04` |
| Full in-UI settings coverage | `11-01` through `11-05` |

## Licence and compliance (`design_overview` §10)

| Requirement | Tasks |
| :-- | :-- |
| `mpv-1.dll` dynamically linked and replaceable | `12-02`, `12-05` |
| LGPL text in every artifact containing mpv | `12-05` |
| mpv source pointer in docs and release manifest | `12-02`, `12-05` |
| `THIRD_PARTY_LICENSES.md`, reachable in-app | `11-07`, `12-05` |
| loxia's own licence (GPL-3.0-or-later) | `00-03`, `12-decisions.md` §8 |

## Branding (`design_overview` §9 — was TBD)

| Requirement | Where |
| :-- | :-- |
| Vector app icon | `assets/logo.svg`, `assets/logo-mono.svg` |
| Design rationale, palette, clear space | `assets/BRANDING.md` |
| ASCII boot banner | `assets/BRANDING.md`, generated by `12-06` |
| Platform icon bundles | `12-06` |

## Requirements with no task

None. Every requirement in `design_overview` is either implemented by a listed task or explicitly
cut in `12-decisions.md` §§2–3, with the reason recorded.
