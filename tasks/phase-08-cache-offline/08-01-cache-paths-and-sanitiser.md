# 08-01 · Cache paths and sanitiser

**Phase:** 08 — Cache & offline · **Agent:** B · **Size:** M
**Prerequisites:** `01-03`
**Reference:** `docs/06-cache-and-offline.md` §§1–3

## Goal
The on-disk layout, cache keys, and the path sanitiser with its traversal guard. This code decides
where files are written and deleted, so it is where a bug destroys user data.

## Files
- `crates/loxia-cache/src/layout.rs`, `error.rs`

## Specification

```
pub struct CacheKey { pub server: ServerId, pub item: ItemId, pub profile: QualityProfile }
impl CacheKey { pub fn filename(&self) -> String; }   // "<item>.<profile>.<ext>"

pub fn sanitize_component(s: &str) -> String;
pub fn download_path(root: &Path, server: &ServerId, t: &Track) -> PathBuf;
pub fn assert_within(root: &Path, target: &Path) -> Result<(), CacheError>;
```

**Cache key** includes the quality profile, so the same track cached at `Direct` and `Med192` are
different files. Cycling quality with `q` must never serve a 96 k file where Direct was requested.

**`sanitize_component`**, in this order:
1. Strip `/ \ : * ? " < > |` and all ASCII control characters.
2. Trim leading and trailing whitespace and `.`.
3. Case-insensitively reject Windows reserved names — `CON PRN AUX NUL COM1..9 LPT1..9` — including
   with an extension (`CON.flac`); prefix with `_` when matched.
4. Truncate to 100 characters at a **grapheme** boundary.
5. An empty result becomes `_`.

**`download_path`** builds `<root>/<server>/<AlbumArtist>/<Album>/<disc>-<track> - <title>.<ext>`,
sanitising every component. Disc and track are zero-padded to 2. A missing album artist uses
`Unknown Artist`; a missing album uses `Unknown Album`.

**`assert_within` is the safety net.** Canonicalise both paths and confirm `root` is a prefix of
`target`. **Every** `remove_file`, `remove_dir`, and write in this crate calls it first. A server
returning a track titled `../../.ssh/authorized_keys` must not be able to write outside the cache
root; this is the code that stops it.

Collision handling: when the target exists and belongs to a different item, append ` (2)`, ` (3)`,
up to 99, then fail with `CacheError::TooManyCollisions`.

## Acceptance
- `sanitize_strips_separators_and_controls`
- `windows_reserved_names_are_prefixed` — table test over all 22 names, mixed case, with and
  without extensions.
- `truncates_at_grapheme_boundary` — a 300-character title of emoji stays valid UTF-8.
- `empty_becomes_underscore`
- `traversal_is_neutralised` — `../../etc/passwd` produces a single flat component.
- `assert_within_rejects_escape` — a symlink pointing outside the root is rejected.
- `assert_within_accepts_nested`
- `cache_key_differs_by_profile`
- `download_path_zero_pads_disc_and_track`
- `missing_album_artist_uses_placeholder`
- `collision_appends_suffix`
- `sanitizer_properties` (proptest) — for arbitrary input the output is non-empty, contains no path
  separator, is valid UTF-8, and is at most 100 characters.

These tests must pass on the Windows CI job as well as Linux — that is the reason this task exists
as its own unit.

## Done when
The global DoD in `tasks/README.md` is satisfied.
