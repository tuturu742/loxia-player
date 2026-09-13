//! `session.json` and `history.json` save/restore (`docs/06-cache-and-offline.md` §8).
//!
//! Server-id/config validation and applying a loaded snapshot onto `AppState` (schema-version
//! checking aside, both need context this module deliberately doesn't have — `load`'s own given
//! signature takes only `&Paths`, no `Config`) live in `loxia_core::reducer::session` instead;
//! see that module's own doc comment.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use loxia_core::paths::Paths;
use loxia_core::state::SessionSnapshot;
use loxia_core::state::queue::HistoryEntry;

use crate::error::CacheError;

/// Bumped whenever `SessionSnapshot`'s own shape changes in a way that makes an old snapshot
/// unsafe to deserialise as the new one (`docs/06-cache-and-offline.md` §8, step 1). `11-03`: a
/// re-export of `loxia_core::state::SESSION_SCHEMA_VERSION`, not a second copy — that crate's own
/// server-switch reducer code needs to stamp a fresh `SessionSnapshot` itself and cannot depend on
/// `loxia-cache` to borrow this constant, so the canonical value lives there instead; re-exporting
/// it here keeps every existing caller of `session::SCHEMA_VERSION` unchanged.
pub const SCHEMA_VERSION: u32 = loxia_core::state::SESSION_SCHEMA_VERSION;

/// "A JSON array capped at 50, newest first" (`docs/06-cache-and-offline.md` §8).
const HISTORY_CAP: usize = 50;

/// Atomic write (`.tmp`, `fsync`, rename) to `paths.session_file()`.
pub fn save(paths: &Paths, s: &SessionSnapshot) -> Result<(), CacheError> {
    write_atomic(&paths.session_file(), s)
}

/// `11-03`: as `save`, but to `paths.session_file_for(server)` — "persist the outgoing session
/// under its own server id" (the server-profile editor's own switch confirmation).
pub fn save_for_server(
    paths: &Paths,
    server: &loxia_core::model::ServerId,
    s: &SessionSnapshot,
) -> Result<(), CacheError> {
    write_atomic(&paths.session_file_for(server), s)
}

/// `11-06`: Settings → Interface → "clear saved session" — removes `paths.session_file()` outright
/// so a relaunch has nothing to restore. A missing file is not an error (nothing was ever saved, or
/// it was already cleared); any other I/O failure is reported so the caller can toast it, unlike
/// every other function in this module, which only ever logs and treats a failure as "start fresh."
pub fn delete(paths: &Paths) -> Result<(), CacheError> {
    match std::fs::remove_file(paths.session_file()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CacheError::Io {
            path: paths.session_file(),
            source,
        }),
    }
}

/// `Ok(None)` for "nothing to restore" — no snapshot ever saved, a `schema_version` mismatch, or a
/// corrupt file — all three are equally "start with an empty queue," never a hard error
/// (`docs/06-cache-and-offline.md` §8: "losing a queue is annoying; failing to start is worse").
/// A schema mismatch or corrupt parse quarantines the file to `session.json.bad`, logged with
/// `warn!`; a missing file needs no quarantine, there being nothing to move.
pub fn load(paths: &Paths) -> Result<Option<SessionSnapshot>, CacheError> {
    load_from(&paths.session_file())
}

/// `11-03`: as `load`, but from `paths.session_file_for(server)` — what a live in-app server
/// switch reads back for the server it's switching *to*, restoring wherever that server's own
/// queue/position was left the last time a switch moved *away* from it. `Ok(None)` (never having
/// switched away from this server before) is exactly as normal as `load`'s own "never saved" case.
pub fn load_for_server(
    paths: &Paths,
    server: &loxia_core::model::ServerId,
) -> Result<Option<SessionSnapshot>, CacheError> {
    load_from(&paths.session_file_for(server))
}

fn load_from(path: &Path) -> Result<Option<SessionSnapshot>, CacheError> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(None);
    };
    match serde_json::from_str::<SessionSnapshot>(&text) {
        Ok(snapshot) if snapshot.schema_version == SCHEMA_VERSION => Ok(Some(snapshot)),
        Ok(snapshot) => {
            tracing::warn!(
                found = snapshot.schema_version,
                expected = SCHEMA_VERSION,
                "session schema version mismatch; discarding"
            );
            quarantine(path);
            Ok(None)
        }
        Err(error) => {
            tracing::warn!(%error, "corrupt session snapshot; quarantining");
            quarantine(path);
            Ok(None)
        }
    }
}

/// Prepends `e` to `paths.history_file()`'s own array (newest first) and caps it at
/// [`HISTORY_CAP`] — a full read-modify-write, unlike the scrobble buffer's line-oriented append,
/// since history is a single bounded JSON array, not an unbounded append-only log
/// (`docs/06-cache-and-offline.md` §8).
pub fn append_history(paths: &Paths, e: &HistoryEntry) -> Result<(), CacheError> {
    let path = paths.history_file();
    let mut entries = read_history(&path);
    entries.push_front(e.clone());
    entries.truncate(HISTORY_CAP);
    let as_vec: Vec<&HistoryEntry> = entries.iter().collect();
    write_atomic(&path, &as_vec)
}

pub fn load_history(paths: &Paths) -> Result<VecDeque<HistoryEntry>, CacheError> {
    Ok(read_history(&paths.history_file()))
}

fn read_history(path: &Path) -> VecDeque<HistoryEntry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return VecDeque::new();
    };
    match serde_json::from_str::<Vec<HistoryEntry>>(&text) {
        Ok(entries) => entries.into_iter().collect(),
        Err(error) => {
            tracing::warn!(%error, "corrupt history.json; starting fresh");
            VecDeque::new()
        }
    }
}

/// `<name>.json` → `<name>.json.bad`, overwriting any previous quarantine — the same
/// `load_or_quarantine` idiom `manifest.rs`/`downloads.rs` already use.
fn quarantine(path: &Path) {
    let bad = path.with_extension("json.bad");
    let _ = std::fs::rename(path, bad);
}

fn write_atomic<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), CacheError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CacheError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(value).expect("value always serialises");

    let mut f = File::create(&tmp).map_err(|source| CacheError::Io {
        path: tmp.clone(),
        source,
    })?;
    f.write_all(&json).map_err(|source| CacheError::Io {
        path: tmp.clone(),
        source,
    })?;
    f.sync_all().map_err(|source| CacheError::Io {
        path: tmp.clone(),
        source,
    })?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::{CacheConfig, QualityProfile};
    use loxia_core::model::ServerId;
    use loxia_core::paths::FixedDirs;
    use loxia_core::state::nav::Tab;
    use loxia_core::state::player::EqState;
    use loxia_core::state::queue::QueueState;
    use loxia_core::test_support::fixtures;
    use tempfile::tempdir;

    fn paths(dir: &Path) -> Paths {
        let dirs = FixedDirs::new(
            Some(dir.join("config")),
            Some(dir.join("cache")),
            Some(dir.join("data")),
            Some(dir.join("state")),
        );
        Paths::resolve(&dirs, &CacheConfig::default()).unwrap()
    }

    fn snapshot(queue: QueueState) -> SessionSnapshot {
        SessionSnapshot {
            schema_version: SCHEMA_VERSION,
            server_id: ServerId::from("srv1"),
            queue,
            position_secs: 42.5,
            active_tab: Tab::Playlists,
            zen_mode: true,
            volume: 60,
            quality_profile: QualityProfile::TranscodeHigh,
            eq: EqState::default(),
            saved_at: fixtures::fixed_epoch(),
        }
    }

    fn some_queue() -> QueueState {
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        let t = fixtures::track("Motion", 1, &alb, &[&a]);
        let mut q = QueueState::default();
        q.entries.push(loxia_core::state::queue::QueueEntry {
            entry_id: loxia_core::model::QueueEntryId(0),
            track: t,
            source: loxia_core::state::queue::QueueSource::Manual,
            availability: loxia_core::state::queue::Availability::Remote,
        });
        q.play_order.push(0);
        q
    }

    #[test]
    fn snapshot_roundtrips() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        let s = snapshot(some_queue());

        save(&paths, &s).unwrap();
        let loaded = load(&paths).unwrap().unwrap();

        assert_eq!(loaded.queue, s.queue);
        assert_eq!(loaded, s);
    }

    /// `11-03`: a live server switch's own "persist the outgoing session under its own server
    /// id" — must not touch the single unsuffixed `session.json` at all.
    #[test]
    fn save_for_server_writes_a_distinct_file_per_server() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        let a = ServerId::from("server-a");
        let b = ServerId::from("server-b");
        let snapshot_a = snapshot(some_queue());
        let mut snapshot_b = snapshot(QueueState::default());
        snapshot_b.server_id = b.clone();

        save_for_server(&paths, &a, &snapshot_a).unwrap();
        save_for_server(&paths, &b, &snapshot_b).unwrap();

        assert!(!paths.session_file().exists());
        let loaded_a = load_for_server(&paths, &a).unwrap().unwrap();
        let loaded_b = load_for_server(&paths, &b).unwrap().unwrap();
        assert_eq!(loaded_a.queue, snapshot_a.queue);
        assert_eq!(loaded_b.queue, snapshot_b.queue);
        assert_ne!(loaded_a.queue, loaded_b.queue);
    }

    #[test]
    fn load_for_server_with_nothing_saved_yet_is_none() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        assert_eq!(
            load_for_server(&paths, &ServerId::from("never-switched-to")).unwrap(),
            None
        );
    }

    #[test]
    fn save_for_server_does_not_collide_with_the_plain_save() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        let plain = snapshot(some_queue());
        let mut scoped = snapshot(QueueState::default());
        scoped.server_id = ServerId::from("server-a");

        save(&paths, &plain).unwrap();
        save_for_server(&paths, &ServerId::from("server-a"), &scoped).unwrap();

        assert_eq!(load(&paths).unwrap().unwrap().queue, plain.queue);
        assert_eq!(
            load_for_server(&paths, &ServerId::from("server-a"))
                .unwrap()
                .unwrap()
                .queue,
            scoped.queue
        );
    }

    #[test]
    fn save_is_atomic() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        save(&paths, &snapshot(some_queue())).unwrap();

        assert!(paths.session_file().exists());
        assert!(!paths.session_file().with_extension("json.tmp").exists());
    }

    #[test]
    fn schema_version_mismatch_discards() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        let mut s = snapshot(some_queue());
        s.schema_version = SCHEMA_VERSION + 1;
        save(&paths, &s).unwrap();

        assert_eq!(load(&paths).unwrap(), None);
        assert!(paths.session_file().with_extension("json.bad").exists());
    }

    #[test]
    fn corrupt_snapshot_is_quarantined_and_startup_succeeds() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        std::fs::create_dir_all(paths.state_root()).unwrap();
        std::fs::write(paths.session_file(), b"{ not json").unwrap();

        let result = load(&paths);
        assert_eq!(
            result.unwrap(),
            None,
            "startup proceeds with an empty queue"
        );
        assert!(paths.session_file().with_extension("json.bad").exists());
    }

    #[test]
    fn history_persists_separately_from_session() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        let entry = HistoryEntry {
            track: fixtures::track(
                "Motion",
                1,
                &fixtures::album("Care", 2019, &fixtures::artist("Boy Harsher")),
                &[&fixtures::artist("Boy Harsher")],
            ),
            played_at: fixtures::fixed_epoch(),
            completed: true,
        };
        append_history(&paths, &entry).unwrap();

        assert!(paths.history_file().exists());
        assert!(
            !paths.session_file().exists(),
            "history and session are separate files"
        );
        assert_eq!(load_history(&paths).unwrap().len(), 1);
    }

    #[test]
    fn clearing_queue_does_not_clear_history() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        let entry = HistoryEntry {
            track: fixtures::track(
                "Motion",
                1,
                &fixtures::album("Care", 2019, &fixtures::artist("Boy Harsher")),
                &[&fixtures::artist("Boy Harsher")],
            ),
            played_at: fixtures::fixed_epoch(),
            completed: true,
        };
        append_history(&paths, &entry).unwrap();

        // A session save with an *empty* queue (as if the user cleared it) must not touch
        // history.json at all — they are entirely independent files.
        save(&paths, &snapshot(QueueState::default())).unwrap();

        assert_eq!(load_history(&paths).unwrap().len(), 1);
    }

    #[test]
    fn delete_removes_the_session_file() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        save(&paths, &snapshot(some_queue())).unwrap();
        assert!(paths.session_file().exists());

        delete(&paths).unwrap();
        assert!(!paths.session_file().exists());
        assert_eq!(load(&paths).unwrap(), None);
    }

    #[test]
    fn delete_on_a_never_saved_session_is_not_an_error() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        delete(&paths).unwrap();
    }

    #[test]
    fn history_capped_at_fifty_on_disk() {
        let dir = tempdir().unwrap();
        let paths = paths(dir.path());
        let a = fixtures::artist("Boy Harsher");
        let alb = fixtures::album("Care", 2019, &a);
        for i in 0..60 {
            let entry = HistoryEntry {
                track: fixtures::track(&format!("Track {i}"), i, &alb, &[&a]),
                played_at: fixtures::fixed_epoch(),
                completed: true,
            };
            append_history(&paths, &entry).unwrap();
        }

        let loaded = load_history(&paths).unwrap();
        assert_eq!(loaded.len(), 50);
        // Newest first: the very last appended (`Track 59`) is at the front.
        assert_eq!(loaded[0].track.name, "Track 59");
    }
}
