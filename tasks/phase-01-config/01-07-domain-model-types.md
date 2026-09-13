# 01-07 · Domain model types

**Phase:** 01 — Config · **Agent:** A · **Size:** M
**Prerequisites:** `00-02`
**Reference:** `docs/02-data-model.md` §§1–2

## Goal
Define every domain type in `loxia-core::model`. These are the vocabulary of the whole workspace, so
they land before `loxia-emby` needs them for DTO conversion.

## Files
- `crates/loxia-core/src/model/ids.rs`, `item.rs`, `audio_meta.rs`, `mod.rs`

## Specification

**Identifiers (`ids.rs`).** Newtype wrappers over `String`, each deriving
`Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize`, with `From<String>`,
`From<&str>`, `as_str()`, and `Display`:
`ItemId`, `ServerId`, `UserId`, `PlaylistId`, `PlaylistEntryId`, `MediaSourceId`.

Plus `QueueEntryId(u64)` — `Copy`, generated from a counter on `QueueState`. It exists because the
same track may appear in a queue more than once, and index-based identity breaks under reordering.

**Never accept a bare `String` id in a public function signature.** That is the whole point of this
module.

**`item.rs`** — `Artist`, `Album`, `Track`, `Genre`, `Folder`, `Playlist`, `MediaItem`,
`SectionHeader`, `SectionKind`, `AlbumRelation`, `LyricStreamRef`, `LyricFormat`, with exactly the
fields listed in `docs/02-data-model.md` §2 (including the `Genre`/`Folder` field lists added there
— both were only ever referenced by name before this task, never given fields). All derive
`Debug, Clone, PartialEq, Serialize, Deserialize`.

`MediaItem` gets:
```
pub fn id(&self) -> Option<&ItemId>;         // None for SectionHeader
pub fn is_selectable(&self) -> bool;         // false for SectionHeader
pub fn display_name(&self) -> &str;
```
`is_selectable` is what the navigation reducer uses to skip headers; putting it on the type rather
than in the reducer means every consumer skips them consistently.

No `sort_key`/`SortKey` method — an earlier draft of this task specified one, but no consuming
task ever calls it: `06-04`'s `sort_items` matches on `MediaItem` variants directly and needs no
generic sort-key abstraction. Track sorting has its own dedicated `compare` in `queue/sort.rs`.
Don't add the method; it would be unused surface.

**`audio_meta.rs`** — `Codec`, `AudioFormat`, `ReplayGainInfo`.
`Codec::from_str` is case-insensitive and maps unknown values to `Codec::Other(String)` rather than
failing. `AudioFormat` gets `pub fn summary(&self) -> String` producing exactly the player-bar form,
e.g. `FLAC 16-bit / 44.1 kHz`, with the bit depth omitted when `None` and the sample rate shown to
one decimal place.

## Acceptance
- `ids_are_distinct_types` — a compile-fail test (`trybuild`, or a documented `// won't compile`
  comment plus a runtime equivalent) showing `ItemId` cannot be passed where `PlaylistId` is
  expected.
- `section_header_is_not_selectable`
- `media_item_id_is_none_for_header`
- `codec_from_str_is_case_insensitive`
- `unknown_codec_becomes_other`
- `audio_format_summary_snapshot` — table test: 16/44.1 FLAC, 24/96 FLAC, MP3 with no bit depth,
  Opus at 48 kHz.
- `model_types_roundtrip_serde` (proptest over `Track`)

## Done when
The global DoD in `tasks/README.md` is satisfied.
