//! Directory scheme, cache keys, and the path sanitiser (`docs/06-cache-and-offline.md` §§1-3).
//!
//! This is the code that decides where files are written and deleted, so a bug here destroys
//! user data — every path a server or a track's own metadata contributes must pass through
//! [`sanitize_component`], and every write or delete must pass through [`assert_within`] first.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use loxia_core::config::{QualityProfile, TargetCodec};
use loxia_core::model::{Codec, ItemId, ServerId, Track};

use crate::error::CacheError;

/// A single grapheme cluster, longer than this, is truncated away — `sanitize_component`'s own
/// step 4.
const MAX_COMPONENT_LEN: usize = 100;

const FORBIDDEN_CHARS: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Case-insensitive; checked against the component's own name **before its first dot**, so
/// `CON.flac` is caught exactly like bare `CON` (`docs/06-cache-and-offline.md` §2 step 3).
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `(server_id, item_id, quality_profile)` — the same track cached at `Direct` and at
/// `TranscodeHigh` are distinct files (`docs/06-cache-and-offline.md` §3); cycling quality with
/// `q` must never serve one profile's file for another.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub server: ServerId,
    pub item: ItemId,
    pub profile: QualityProfile,
}

impl CacheKey {
    /// `"<item>.<profile>.<ext>"` — `<profile>` alone already makes every profile's file distinct
    /// regardless of what `<ext>` resolves to, so a mid-session change to the global
    /// `transcode.target_codec` setting (`docs/12-decisions.md`) can never silently reuse a
    /// differently-encoded file under an unchanged name.
    ///
    /// `ext` is supplied by the caller because the key alone cannot know it: under `Direct` the
    /// bytes are the source file's, whose codec lives on the `Track`, not in the key. See
    /// [`cache_extension`].
    pub fn filename(&self, ext: &str) -> String {
        format!("{}.{}.{}", self.item, profile_slug(self.profile), ext)
    }
}

/// The extension a cached file should carry, i.e. what its bytes actually are.
///
/// Under `Direct` the server streams the source file untouched, so the extension is the **track's
/// own** codec — every cached file was previously named `.flac` regardless, because the extension
/// was derived from the quality profile alone and `Direct` was assumed to mean FLAC
/// (`docs/12-decisions.md`). Under a transcode profile the bytes are whatever
/// `transcode.target_codec` asked for; the profile only picks the bitrate, so it cannot name the
/// container either.
pub fn cache_extension(profile: QualityProfile, source: &Codec, target: TargetCodec) -> String {
    match profile {
        QualityProfile::Direct => codec_extension(source),
        _ => target_codec_extension(target).to_string(),
    }
}

/// Must match `loxia_emby::stream`'s own profile-to-extension mapping exactly — that is the
/// request this file is the response to, so a disagreement would name an `.mp3` file `.opus`.
/// Duplicated rather than shared because `loxia-cache` cannot depend on `loxia-emby`; both sides
/// carry the same table under test, the same arrangement `clamp_eq_gain` already uses.
fn target_codec_extension(codec: TargetCodec) -> &'static str {
    match codec {
        TargetCodec::Mp3 => "mp3",
        TargetCodec::Aac => "m4a",
        TargetCodec::Opus => "opus",
    }
}

fn profile_slug(profile: QualityProfile) -> &'static str {
    match profile {
        QualityProfile::Direct => "direct",
        QualityProfile::TranscodeHigh => "transcode_high",
        QualityProfile::TranscodeMed => "transcode_med",
        QualityProfile::TranscodeLow => "transcode_low",
    }
}

fn codec_extension(codec: &Codec) -> String {
    match codec {
        Codec::Flac => "flac".to_string(),
        Codec::Alac => "m4a".to_string(),
        Codec::Mp3 => "mp3".to_string(),
        Codec::Aac => "aac".to_string(),
        Codec::Opus => "opus".to_string(),
        Codec::Vorbis => "ogg".to_string(),
        Codec::Wav => "wav".to_string(),
        Codec::Other(name) => name.to_ascii_lowercase(),
    }
}

/// Where a cached track lives **relative to the tracks root**:
/// `<server>/<AlbumArtist>/<Album>/<disc>-<track> - <title>.<profile>.<ext>`.
///
/// The rolling cache used to be a flat `<server>/<item>.<profile>.<ext>`, which made the cache
/// directory unreadable — you could not tell which artists or albums had actually been pulled down
/// (`docs/12-decisions.md`). The directory shape now mirrors `download_path`'s, so both tiers are
/// browsable the same way; the `<profile>` segment stays in the filename because the same track at
/// two quality profiles is two distinct files.
///
/// Two different items can still land on one name (the same album in two libraries, say), so the
/// caller resolves collisions via [`resolve_collision`] against its own manifest — the path alone
/// is a proposal, not a guarantee.
pub fn cache_relative_path(
    server: &ServerId,
    track: &Track,
    profile: QualityProfile,
    ext: &str,
) -> PathBuf {
    let title = format!(
        "{:02}-{:02} - {}",
        track.disc_number.unwrap_or(1),
        track.track_number.unwrap_or(0),
        track.name
    );
    PathBuf::from(sanitize_component(server.as_str()))
        .join(sanitize_component(album_artist_of(track)))
        .join(sanitize_component(album_of(track)))
        .join(format!(
            "{}.{}.{ext}",
            sanitize_component(&title),
            profile_slug(profile)
        ))
}

/// Shared by [`cache_relative_path`] and [`download_path`] so the two tiers can never disagree
/// about which folder a track belongs in.
fn album_artist_of(t: &Track) -> &str {
    t.album_artist_names
        .first()
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Unknown Artist")
}

fn album_of(t: &Track) -> &str {
    if t.album_name.trim().is_empty() {
        "Unknown Album"
    } else {
        &t.album_name
    }
}

/// Every user- or server-derived path component passes through this before touching a filesystem
/// path, in this exact order (`docs/06-cache-and-offline.md` §2):
/// 1. Strip `/ \ : * ? " < > |` and all ASCII control characters.
/// 2. Trim leading/trailing whitespace and `.`.
/// 3. Case-insensitively reject Windows reserved names, prefixing with `_` when matched.
/// 4. Truncate to 100 characters at a grapheme boundary.
/// 5. An empty result becomes `_`.
pub fn sanitize_component(s: &str) -> String {
    let stripped: String = s
        .chars()
        .filter(|c| !FORBIDDEN_CHARS.contains(c) && !c.is_ascii_control())
        .collect();

    let trimmed = stripped
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_string();

    let reserved_handled = reject_reserved_name(trimmed);

    let truncated: String = reserved_handled
        .graphemes(true)
        .take(MAX_COMPONENT_LEN)
        .collect();

    if truncated.is_empty() {
        "_".to_string()
    } else {
        truncated
    }
}

fn reject_reserved_name(s: String) -> String {
    let base = s.split('.').next().unwrap_or("");
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(base)) {
        format!("_{s}")
    } else {
        s
    }
}

/// `<root>/<server>/<AlbumArtist>/<Album>/<disc>-<track> - <title>.<ext>`
/// (`docs/06-cache-and-offline.md` §1) — every path-derived component sanitised individually
/// (never the assembled path as one string, which would let a track title containing a literal
/// `/` merge two components together). Disc and track are zero-padded to 2; a missing album
/// artist or album name falls back to `Unknown Artist`/`Unknown Album`. The extension follows the
/// track's own real codec (this is the permanent-downloads path, always fetched via the `Direct`
/// profile — `transcode.download_uncompressed`, `docs/06-cache-and-offline.md` §5 — so the actual
/// codec is known precisely, unlike `CacheKey::filename`'s own fixed per-profile guess for the
/// rolling cache).
pub fn download_path(root: &Path, server: &ServerId, t: &Track) -> PathBuf {
    let album_artist = album_artist_of(t);
    let album = album_of(t);
    let disc = t.disc_number.unwrap_or(1);
    let track_number = t.track_number.unwrap_or(0);
    let ext = codec_extension(&t.format.codec);
    let title = format!("{disc:02}-{track_number:02} - {}", t.name);

    root.join(sanitize_component(server.as_str()))
        .join(sanitize_component(album_artist))
        .join(sanitize_component(album))
        .join(format!("{}.{ext}", sanitize_component(&title)))
}

/// The safety net: canonicalises both `root` and `target` (climbing to the nearest existing
/// ancestor for `target`, which typically doesn't exist yet — it's about to be written) and
/// confirms `root` is a prefix of the result. A server returning a track titled
/// `../../.ssh/authorized_keys` must not be able to write outside the cache root; this is the
/// code that stops it. Every `remove_file`, `remove_dir`, and write anywhere in this crate calls
/// this first.
pub fn assert_within(root: &Path, target: &Path) -> Result<(), CacheError> {
    let root_canon = canonicalize(root)?;
    let target_canon = canonicalize_best_effort(target)?;
    if target_canon.starts_with(&root_canon) {
        Ok(())
    } else {
        Err(CacheError::PathEscapesRoot {
            root: root_canon,
            target: target_canon,
        })
    }
}

fn canonicalize(path: &Path) -> Result<PathBuf, CacheError> {
    path.canonicalize().map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Canonicalises the deepest existing ancestor of `path` (resolving any symlink along that real
/// portion) and rejoins whatever doesn't exist yet literally on top — `path.canonicalize()` alone
/// fails outright on a not-yet-created target, which is the common case here.
fn canonicalize_best_effort(path: &Path) -> Result<PathBuf, CacheError> {
    if let Ok(canon) = path.canonicalize() {
        return Ok(canon);
    }
    let Some(parent) = path.parent() else {
        return Ok(path.to_path_buf());
    };
    let parent_canon = canonicalize_best_effort(parent)?;
    match path.file_name() {
        Some(name) => Ok(parent_canon.join(name)),
        None => Ok(parent_canon),
    }
}

/// Inserts a ` (n)` disambiguator immediately before the target's own extension (matching how
/// every desktop OS already resolves a save-as collision) — `"foo.flac"` at `n = 2` becomes
/// `"foo (2).flac"`. `pub(crate)`: the rolling cache disambiguates against its **manifest** rather
/// than the filesystem ([`resolve_collision`] short-circuits on a path that doesn't exist yet,
/// which is exactly the case when a row has been reserved but nothing downloaded), so it needs the
/// naming rule without that guard.
pub(crate) fn suffixed(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let new_name = match path.extension() {
        Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
        None => format!("{stem} ({n})"),
    };
    match path.parent() {
        Some(parent) => parent.join(new_name),
        None => PathBuf::from(new_name),
    }
}

/// Resolves a filename collision at `path`: returned unchanged if nothing is there yet, or if
/// something is but `belongs_to_other` (a caller-supplied check — only the caller, via a
/// `.loxia.json` sidecar or the cache manifest, can tell "already this exact item" apart from "a
/// genuine collision") says it's actually the *same* item. Otherwise tries ` (2)` through ` (99)`
/// in turn, failing with [`CacheError::TooManyCollisions`] if every one of those is also taken by
/// something else.
pub fn resolve_collision(
    path: &Path,
    belongs_to_other: impl Fn(&Path) -> bool,
) -> Result<PathBuf, CacheError> {
    if !path.exists() || !belongs_to_other(path) {
        return Ok(path.to_path_buf());
    }
    for n in 2..=99 {
        let candidate = suffixed(path, n);
        if !candidate.exists() || !belongs_to_other(&candidate) {
            return Ok(candidate);
        }
    }
    Err(CacheError::TooManyCollisions(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::test_support::fixtures;
    use proptest::prelude::*;
    use std::fs;
    use tempfile::tempdir;

    /// Every cached file used to be named `.flac`, because the extension came from the quality
    /// profile alone and `Direct` was taken to mean FLAC. `Direct` actually streams the source
    /// file untouched, so the extension is whatever that file already is
    /// (`docs/12-decisions.md`).
    #[test]
    fn direct_caching_keeps_the_sources_own_extension() {
        for (codec, expected) in [
            (Codec::Flac, "flac"),
            (Codec::Mp3, "mp3"),
            (Codec::Opus, "opus"),
            (Codec::Alac, "m4a"),
            (Codec::Vorbis, "ogg"),
            (Codec::Wav, "wav"),
            (Codec::Other("dsf".to_string()), "dsf"),
        ] {
            assert_eq!(
                cache_extension(QualityProfile::Direct, &codec, TargetCodec::Mp3),
                expected,
                "a {codec:?} source cached at Direct must stay a .{expected}"
            );
        }
    }

    /// Under a transcode profile the bytes are whatever `transcode.target_codec` asked for. The
    /// profile chooses the *bitrate*, so it can't name the container — the old mapping had
    /// `TranscodeMed` meaning `.opus` no matter what was actually requested.
    #[test]
    fn transcoded_caching_follows_the_requested_codec() {
        for profile in [
            QualityProfile::TranscodeHigh,
            QualityProfile::TranscodeMed,
            QualityProfile::TranscodeLow,
        ] {
            for (target, expected) in [
                (TargetCodec::Mp3, "mp3"),
                (TargetCodec::Aac, "m4a"),
                (TargetCodec::Opus, "opus"),
            ] {
                assert_eq!(
                    cache_extension(profile, &Codec::Flac, target),
                    expected,
                    "{profile:?} requesting {target:?} must produce .{expected}"
                );
            }
        }
    }

    /// The extension the cache writes has to match the one the stream URL requested, or the file
    /// is mislabelled on disk. `loxia-emby` owns that mapping; this is the mirror of it.
    #[test]
    fn transcode_extensions_match_the_stream_request() {
        assert_eq!(target_codec_extension(TargetCodec::Mp3), "mp3");
        assert_eq!(target_codec_extension(TargetCodec::Aac), "m4a");
        assert_eq!(target_codec_extension(TargetCodec::Opus), "opus");
    }

    /// The rolling cache was a flat `<server>/<item>.<profile>.<ext>`, so the directory said
    /// nothing about which artists or albums had been pulled down (`docs/12-decisions.md`).
    #[test]
    fn cached_tracks_are_filed_under_album_artist_and_album() {
        let artist = fixtures::artist("Boy Harsher");
        let album = fixtures::album("Care", 2019, &artist);
        let track = fixtures::track("Motion", 3, &album, &[&artist]);

        let path = cache_relative_path(
            &ServerId::from("srv"),
            &track,
            QualityProfile::Direct,
            "flac",
        );

        assert_eq!(
            path,
            PathBuf::from("srv/Boy Harsher/Care/01-03 - Motion.direct.flac"),
            "the directories are the point: they name what has been cached"
        );
    }

    /// The same track at two profiles stays two files, which is what stops `q` serving a
    /// transcode where Direct was asked for.
    #[test]
    fn each_quality_profile_gets_its_own_file() {
        let artist = fixtures::artist("Boy Harsher");
        let album = fixtures::album("Care", 2019, &artist);
        let track = fixtures::track("Motion", 3, &album, &[&artist]);
        let at = |profile, ext| cache_relative_path(&ServerId::from("srv"), &track, profile, ext);

        assert_ne!(
            at(QualityProfile::Direct, "flac"),
            at(QualityProfile::TranscodeHigh, "mp3")
        );
    }

    /// Path components come from server metadata, so every one of them has to be sanitised
    /// individually — a track title containing `/` must not silently become a directory.
    #[test]
    fn cache_path_components_are_sanitised_individually() {
        let artist = fixtures::artist("AC/DC");
        let album = fixtures::album("Back/Slash", 1980, &artist);
        let mut track = fixtures::track("Hells/Bells", 1, &album, &[&artist]);
        track.album_artist_names = vec!["AC/DC".to_string()];
        track.album_name = "Back/Slash".to_string();

        let path = cache_relative_path(
            &ServerId::from("srv"),
            &track,
            QualityProfile::Direct,
            "flac",
        );

        assert_eq!(
            path.components().count(),
            4,
            "server / artist / album / file — no extra levels smuggled in: {path:?}"
        );
        assert!(path.to_string_lossy().contains("ACDC"));
    }

    /// A track with no album artist or album still has to land somewhere predictable.
    #[test]
    fn missing_album_metadata_falls_back_to_named_folders() {
        let artist = fixtures::artist("Someone");
        let album = fixtures::album("Something", 2000, &artist);
        let mut track = fixtures::track("Untitled", 1, &album, &[&artist]);
        track.album_artist_names = Vec::new();
        track.album_name = String::new();

        let path = cache_relative_path(
            &ServerId::from("srv"),
            &track,
            QualityProfile::Direct,
            "flac",
        );
        assert!(
            path.starts_with("srv/Unknown Artist/Unknown Album"),
            "{path:?}"
        );
    }

    #[test]
    fn sanitize_strips_separators_and_controls() {
        assert_eq!(sanitize_component("a/b\\c:d*e?f\"g<h>i|j"), "abcdefghij");
        assert_eq!(sanitize_component("a\u{0}b\u{1f}c\u{7f}"), "abc");
    }

    #[test]
    fn windows_reserved_names_are_prefixed() {
        let names = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        assert_eq!(names.len(), 22);
        for name in names {
            assert_eq!(sanitize_component(name), format!("_{name}"));
            let lower = name.to_ascii_lowercase();
            assert_eq!(sanitize_component(&lower), format!("_{lower}"));
            let mixed = format!(
                "{}{}",
                name.chars().next().unwrap(),
                name[1..].to_ascii_lowercase()
            );
            assert_eq!(sanitize_component(&mixed), format!("_{mixed}"));
            assert_eq!(
                sanitize_component(&format!("{name}.flac")),
                format!("_{name}.flac")
            );
        }
        // A non-reserved name sharing a prefix must not be caught.
        assert_eq!(sanitize_component("CONcert"), "CONcert");
    }

    #[test]
    fn truncates_at_grapheme_boundary() {
        let title: String = std::iter::repeat_n("🎵", 300).collect();
        let result = sanitize_component(&title);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
        assert_eq!(result.graphemes(true).count(), 100);
    }

    #[test]
    fn empty_becomes_underscore() {
        assert_eq!(sanitize_component(""), "_");
        assert_eq!(sanitize_component("   "), "_");
        assert_eq!(sanitize_component("..."), "_");
        assert_eq!(sanitize_component("/\\:*?\"<>|"), "_");
    }

    #[test]
    fn traversal_is_neutralised() {
        // Leading `../../` dots and slashes are stripped (slashes outright, dots by the
        // leading-trim step) into one flat component with neither a separator nor a `..` segment.
        let result = sanitize_component("../../etc/passwd");
        assert_eq!(result, "etcpasswd");
        assert!(!result.contains('/'));
        assert!(!result.contains(".."));
        assert!(!sanitize_component("..\\..\\windows").contains('\\'));
    }

    #[test]
    fn assert_within_accepts_nested() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        let target = nested.join("track.flac");
        assert!(assert_within(dir.path(), &target).is_ok());
    }

    #[test]
    fn assert_within_rejects_escape() {
        let dir = tempdir().unwrap();
        let cache_root = dir.path().join("cache");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&cache_root).unwrap();
        fs::create_dir_all(&outside).unwrap();

        // A symlink inside the root pointing outside it.
        let link = cache_root.join("escape");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, &link).unwrap();

        let target = link.join("secret.flac");
        let result = assert_within(&cache_root, &target);
        assert!(result.is_err(), "a symlink escape must be rejected");
    }

    #[test]
    fn cache_key_differs_by_profile() {
        let server = ServerId::from("srv1");
        let item = ItemId::from("item1");
        let direct = CacheKey {
            server: server.clone(),
            item: item.clone(),
            profile: QualityProfile::Direct,
        };
        let high = CacheKey {
            server,
            item,
            profile: QualityProfile::TranscodeHigh,
        };
        assert_ne!(direct.filename("flac"), high.filename("flac"));
        assert_ne!(direct, high);
    }

    fn track_with(
        album_artist: Option<&str>,
        album: Option<&str>,
        disc: Option<u32>,
        track_number: Option<u32>,
    ) -> Track {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album(album.unwrap_or("Care"), 2019, &a);
        let mut t = fixtures::track("Motion", 1, &alb, &[&a]);
        t.album_artist_names = album_artist
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();
        if album.is_none() {
            t.album_name = String::new();
        }
        t.disc_number = disc;
        t.track_number = track_number;
        t
    }

    #[test]
    fn download_path_zero_pads_disc_and_track() {
        let t = track_with(Some("Boy Harsher"), Some("Care"), Some(1), Some(3));
        let root = Path::new("/cache");
        let server = ServerId::from("srv1");
        let path = download_path(root, &server, &t);
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            "01-03 - Motion.flac"
        );
    }

    #[test]
    fn missing_album_artist_uses_placeholder() {
        let t = track_with(None, Some("Care"), Some(1), Some(1));
        let path = download_path(Path::new("/cache"), &ServerId::from("srv1"), &t);
        assert!(path.to_string_lossy().contains("Unknown Artist"));

        let t2 = track_with(Some("Boy Harsher"), None, Some(1), Some(1));
        let path2 = download_path(Path::new("/cache"), &ServerId::from("srv1"), &t2);
        assert!(path2.to_string_lossy().contains("Unknown Album"));
    }

    #[test]
    fn collision_appends_suffix() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("track.flac");
        fs::write(&path, b"other item's data").unwrap();

        let resolved = resolve_collision(&path, |_| true).unwrap();
        assert_eq!(resolved, dir.path().join("track (2).flac"));

        // Belonging to the *same* item is not a collision at all.
        let same = resolve_collision(&path, |_| false).unwrap();
        assert_eq!(same, path);
    }

    #[test]
    fn collision_gives_up_after_99() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("track.flac");
        fs::write(&path, b"x").unwrap();
        for n in 2..=99 {
            fs::write(dir.path().join(format!("track ({n}).flac")), b"x").unwrap();
        }
        let result = resolve_collision(&path, |_| true);
        assert!(matches!(result, Err(CacheError::TooManyCollisions(_))));
    }

    proptest! {
        #[test]
        fn sanitizer_properties(s in ".{0,300}") {
            let result = sanitize_component(&s);
            prop_assert!(!result.is_empty());
            prop_assert!(!result.contains('/'));
            prop_assert!(!result.contains('\\'));
            prop_assert!(std::str::from_utf8(result.as_bytes()).is_ok());
            prop_assert!(result.graphemes(true).count() <= MAX_COMPONENT_LEN);
        }
    }
}
