# 06 — Cache, Downloads & Offline (`loxia-cache`)

## 1. Directory layout

```
cache root      Linux ~/.cache/loxia-player   macOS ~/Library/Caches/loxia   Windows %LOCALAPPDATA%\loxia\cache
├── cache_index.json
├── cache.lock
├── tracks/<server_id>/<item_id>.<profile>.<ext>
└── images/<item_id>_<size>_<tag>.jpg

data root       Linux ~/.local/share/loxia-player    macOS ~/Library/Application Support/loxia-player    Windows %APPDATA%\loxia
└── downloads/
    ├── downloads_index.json
    └── <server_id>/<AlbumArtist>/<Album>/<disc>-<track> - <title>.<ext>
        └── (+ a .loxia.json sidecar and cover.jpg per album)

state root      Linux ~/.local/state/loxia-player    (macOS/Windows: same as data root)
├── session.json  history.json  scrobbles.json  loxia-player.log
```

Downloads use **human-readable paths** deliberately — users expect to find and copy their offline
music. The cache uses opaque ids because nobody browses it.

`cache.download_dir` and `cache.cache_dir` default to `"auto"`, resolving as above. An explicit path
overrides; it is checked for writability at startup and falls back with a warning if not.

## 2. Path sanitiser (`layout.rs`)

Every user- or server-derived path component passes through `sanitize_component`:

1. Strip `/ \ : * ? " < > |` and all ASCII control characters.
2. Trim leading/trailing whitespace and dots.
3. Reject and replace Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
   `LPT1`–`LPT9`), case-insensitively, including with an extension.
4. Truncate to 100 **characters** at a grapheme boundary — never mid-codepoint.
5. Empty result → `"_"`.
6. Collisions get a ` (2)`, ` (3)` suffix.

**Traversal guard.** Before any `remove_file`, `remove_dir`, or write, canonicalise the target and
assert the canonical cache/download root is a prefix. This is asserted in code, not just tested.
A path escaping the root is a bug that deletes user data.

## 3. Cache key

`(server_id, item_id, quality_profile)`. `quality_profile` is `config::QualityProfile` — `Direct`,
`TranscodeHigh`, `TranscodeMed`, or `TranscodeLow`. The same track at `Direct` and at
`TranscodeHigh` are distinct entries — cycling quality with `q` must never serve a transcoded file
when Direct was requested.

Files are laid out as `tracks/<server>/<AlbumArtist>/<Album>/<disc>-<track> - <title>.<profile>.<ext>`,
mirroring the downloads tier so the cache directory is browsable — you can see which artists and
albums have actually been pulled down. Two different items can propose the same name (the same album
present in two libraries), so the cache disambiguates with a ` (n)` suffix against its **manifest**,
not the filesystem: a row is reserved before any byte is written, so the clashing path routinely
doesn't exist yet. Reconciliation walks the tree recursively and prunes directories it empties.

The extension names **what the bytes actually are**, and so cannot be derived from the profile
alone (`layout::cache_extension`): under `Direct` the server streams the source file untouched, so
it is the track's own codec; under a transcode profile it is `transcode.target_codec`'s container
(`mp3`/`m4a`/`opus`), the profile having chosen only the bitrate. The path is computed once when
the entry is created and then stored on the manifest row, so an entry written under an older
scheme keeps resolving to its own file.

## 4. Rolling LRU (`lru.rs`, `manifest.rs`)

### Manifest entry
`{ key, path, bytes, created_at, last_access, complete: bool, duration_secs }`

### Behaviour
- **Who downloads.** mpv fetches its own stream over HTTP and we cannot cheaply tee it, so the
  decision is made at play time: if the key is cached and complete, hand mpv a `file://` path;
  otherwise hand it the network URL **and** start an independent background fetch of the same URL.
  A first play costs double bandwidth; in exchange the logic is trivially correct, resumable, and
  shared with the pinning path. Background fetching is limited to one concurrent track so it never
  competes with playback, and is disabled by `cache.prefetch_on_play = false`.
- **Read-ahead** (`cache.prefetch_next`, default `1`) is the *other* half: the next N upcoming
  queue entries are fetched into the same rolling cache, sequentially, on a task that is deliberately
  kept apart from the play-time fetch above — it can neither cancel that fetch nor be cancelled by
  it, and a new run supersedes the previous one because the queue has moved on. It only ever touches
  tracks *other* than the one playing; the current track is the play-time fetch's own business.
  Measured against a live server before being defaulted on: a concurrent fetch never interrupted a
  held-open stream, in either profile (see `12-decisions.md`, 2026-08-06).
- **Eviction** runs when total bytes exceed `cache.rolling_max_gb`, dropping least-recently-accessed
  complete entries until usage reaches 90 % of the limit. The hysteresis prevents thrashing.
  **Never evict:** the playing track, the preloaded next track, or a `.part` file younger than five
  minutes. Downloads are a separate tier and are never touched.
- **Startup reconciliation:** scan `tracks/`, drop manifest rows with no file, delete files with no
  row, delete stale `.part` files, and log the reclaimed bytes.
- The manifest is written atomically and debounced to at most one write per second.

## 5. Permanent downloads (`downloads.rs`)

Triggered by `d` on a Track, Album, Artist, or Playlist → `Effect::Cache(PinDownload { scope })`.

- Expand the scope to a track list (fetching if not already loaded), then enqueue.
- **Bounded concurrency of 2**, resumable through HTTP `Range` against `.part` files, reporting
  `Event::DownloadProgress { id, done, total }` so the UI can show per-item progress and a global
  "↓3" badge in the header.
- `transcode.download_uncompressed = true` (the default) forces the `Direct` profile regardless of
  the active quality profile. Pinning is for keeping, not for saving bytes.
- Alongside each audio file, write a `.loxia.json` sidecar holding the full `Track` metadata, and
  fetch `cover.jpg` once per album. **This is what makes offline browsing work with no server.**
- `downloads_index.json` maps `item_id → { path, profile, bytes, downloaded_at, sidecar }`.
- Unpinning (`d` again) deletes the files and prunes newly empty directories.
- Downloads have **no size cap** and are never auto-evicted. Settings shows the total.

## 6. Offline mode

```
Online ──(2 consecutive network failures)──► Offline
Offline ──(probe succeeds)──► Reconnecting ──(scrobbles drained)──► Online
```
Probe: `GET /System/Info/Public` on a jittered 5 s → 30 s backoff, plus an immediate probe whenever
the user takes an action that needs the network.

While `Offline`:
- Browse columns are served from `offline_index.rs`, an in-memory tree built from download sidecars
  at startup.
- Tabs that cannot be served offline show an explanatory empty state, never a spinner that never
  resolves.
- Items that are neither cached nor downloaded render dimmed with a `⚠` prefix and refuse to queue,
  with the toast "not available offline".
- The header shows an `OFFLINE` badge and the pending scrobble count.

## 7. Scrobble buffer (`scrobble.rs`)

Append-only JSON-lines file. Each record:
`{ kind: Start|Progress|Stopped|Played, item_id, server_id, position_ticks, occurred_at,
play_session_id }`

- On replay, `Progress` records are **collapsed** — only the last per `play_session_id` is kept.
  Replaying hundreds of no-op progress updates wastes the reconnect window.
- Replay is chronological. A failure leaves the record in the buffer for the next reconnect.
- Capped at 5000 records. On overflow, drop the oldest `Progress` records first; **never** drop
  `Played`, which is the one the user can actually observe in Emby.
- Deduplicate on `(item_id, play_session_id, kind)`.

## 8. Session and history (`session.rs`)

`SessionSnapshot` is saved on quit, on track change, and every 15 s while playing.

Restore at startup when `ui.restore_session = true`:
1. Validate `schema_version` and that `server_id` matches `active_server`; otherwise discard.
2. Rebuild `QueueState` from the snapshot — **do not refetch tracks at startup**.
3. Restore active tab, Zen state, volume, quality profile, and EQ.
4. **Restore paused, at the saved position.** The player bar shows `⏸ resumed at 01:24`. Auto-play
   on launch is startling and steals the user's audio device; it is opt-in via
   `ui.restore_autoplay = true`.
5. A corrupt snapshot is renamed to `session.json.bad` and startup continues clean.

`history.json` is a separate ring of the last 50 completed plays, appended on completion using the
same ≥90 % / ≥4 min threshold as playback reporting. Keeping it out of the session file means
clearing the queue does not clear history.

## 9. Failure and safety rules

- **Every disk write is atomic**: write `.tmp` in the same directory, `fsync`, rename. A power cut
  must never leave a corrupt index.
- Disk full (`ENOSPC`): stop caching, toast once, keep playing — streaming still works.
- Two instances sharing a cache: take an advisory lock (`cache.lock`, via `fs4`). If it is held,
  run in **read-only cache mode** and log it. Refusing to start would be worse.
- A corrupt `cache_index.json` is quarantined and rebuilt by scanning the directory.

## 10. Tests

- LRU: fill past the limit and assert eviction order; assert the playing and preloaded tracks are
  never evicted; assert the 90 % hysteresis target.
- Path sanitiser (proptest): output never escapes the root, is never empty, and is never a Windows
  reserved name — for inputs including `../../etc/passwd`, `CON`, 300-character titles, and emoji.
- Resume: truncate a `.part`, re-run, assert a `Range` request was made and the final file is
  byte-exact.
- Offline: with wiremock returning connection errors, assert the transition after two failures, that
  queueing an unavailable track is refused, and that scrobbles buffer and then drain in order.
- Session: snapshot → serialise → deserialise → identical `QueueState`; a corrupt file is
  quarantined and startup succeeds.
