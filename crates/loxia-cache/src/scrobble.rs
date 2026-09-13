//! Offline playback-event queue: buffers `PlaybackReport`s while offline and replays them on
//! reconnect, so play counts survive a network drop (`docs/06-cache-and-offline.md` §7).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use loxia_core::model::{ItemId, PlaySessionId, PlaybackReport, ServerId};

use crate::error::CacheError;

/// Records past this are trimmed — oldest `Progress` first, `Played` never
/// (`docs/06-cache-and-offline.md` §7: "the only one whose loss a user can observe in Emby").
const MAX_RECORDS: usize = 5000;

/// One line of `scrobbles.json`: `{ kind, item_id, server_id, position_ticks, occurred_at,
/// play_session_id }` — `kind`/`item_id`/`position_ticks`/`play_session_id` all live inside
/// `report` itself (`PlaybackReport`'s own variants), flattened into the same JSON object rather
/// than nested, so the on-disk shape matches the task's own literal field list directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScrobbleRecord {
    server: ServerId,
    occurred_at: Timestamp,
    #[serde(flatten)]
    report: PlaybackReport,
}

/// What `PlaybackReport`'s four variants collapse to for grouping — `(item, session, kind)` is
/// this task's own deduplication/collapsing key. `Played` carries no session at all, so it groups
/// purely on `(item, kind)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    item: ItemId,
    session: Option<PlaySessionId>,
    kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Start,
    Progress,
    Stopped,
    Played,
}

fn key_of(report: &PlaybackReport) -> Key {
    match report {
        PlaybackReport::Start { item, session } => Key {
            item: item.clone(),
            session: Some(session.clone()),
            kind: Kind::Start,
        },
        PlaybackReport::Progress { item, session, .. } => Key {
            item: item.clone(),
            session: Some(session.clone()),
            kind: Kind::Progress,
        },
        PlaybackReport::Stopped { item, session, .. } => Key {
            item: item.clone(),
            session: Some(session.clone()),
            kind: Kind::Stopped,
        },
        PlaybackReport::Played { item } => Key {
            item: item.clone(),
            session: None,
            kind: Kind::Played,
        },
    }
}

fn is_progress(report: &PlaybackReport) -> bool {
    matches!(report, PlaybackReport::Progress { .. })
}

fn is_played(report: &PlaybackReport) -> bool {
    matches!(report, PlaybackReport::Played { .. })
}

/// Buffers `PlaybackReport`s to `scrobbles.json` (append-only JSON Lines) and replays them once
/// the connection returns. Loaded once at startup so pending records survive a restart while
/// still offline.
pub struct ScrobbleBuffer {
    path: PathBuf,
    server: ServerId,
    records: Vec<ScrobbleRecord>,
}

impl ScrobbleBuffer {
    /// Opens (creating if absent) `root/scrobbles.json`. A corrupt or truncated line is skipped
    /// with a `warn!` rather than losing every other pending record.
    pub fn open(root: &Path, server: ServerId) -> Result<Self, CacheError> {
        std::fs::create_dir_all(root).map_err(|source| CacheError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = root.join("scrobbles.json");
        let records = load(&path);
        Ok(ScrobbleBuffer {
            path,
            server,
            records,
        })
    }

    pub fn pending(&self) -> usize {
        self.records.len()
    }

    /// Appends one record (a single line, no read-modify-write) and enforces [`MAX_RECORDS`] —
    /// only on overflow does this need to rewrite the whole file, since the in-memory mirror and
    /// the on-disk file must then agree on which record(s) got dropped.
    pub fn append(&mut self, r: PlaybackReport, at: Timestamp) -> Result<(), CacheError> {
        let record = ScrobbleRecord {
            server: self.server.clone(),
            occurred_at: at,
            report: r,
        };
        self.records.push(record);

        if self.records.len() > MAX_RECORDS {
            self.enforce_cap();
            return self.rewrite();
        }

        self.append_line(self.records.last().expect("just pushed"))
    }

    /// Drops the oldest `Progress` records first; once none remain, the oldest non-`Played`
    /// record; `Played` itself is never removed, even if the cap is still technically exceeded
    /// afterwards — a pathological case (5000 records with none droppable) this priority order
    /// accepts rather than violates "never drop `Played`" (`docs/12-decisions.md`).
    fn enforce_cap(&mut self) {
        while self.records.len() > MAX_RECORDS {
            let idx = self
                .records
                .iter()
                .position(|r| is_progress(&r.report))
                .or_else(|| self.records.iter().position(|r| !is_played(&r.report)));
            match idx {
                Some(idx) => {
                    self.records.remove(idx);
                }
                None => break,
            }
        }
    }

    /// Replays pending records: collapses to the most recent record per `(item, session, kind)`
    /// (a `Progress` history collapses to its last entry; every distinct `Start`/`Stopped`/
    /// `Played` key survives), replays chronologically by `occurred_at`, and stops at the first
    /// `send` that returns `false` — a failure almost certainly means the connection dropped
    /// again, so continuing to the next record would spend the whole reconnect window failing
    /// identically. Everything successfully sent (and every other raw record sharing its key,
    /// which collapsing had already superseded) is removed; whatever's left, including anything
    /// never attempted, stays for the next drain.
    pub async fn drain<F, Fut>(&mut self, send: F) -> Result<usize, CacheError>
    where
        F: Fn(PlaybackReport) -> Fut,
        Fut: Future<Output = bool>,
    {
        let mut latest: HashMap<Key, usize> = HashMap::new();
        for (i, record) in self.records.iter().enumerate() {
            let key = key_of(&record.report);
            let supersedes = match latest.get(&key) {
                Some(&existing) => record.occurred_at >= self.records[existing].occurred_at,
                None => true,
            };
            if supersedes {
                latest.insert(key, i);
            }
        }
        let mut order: Vec<usize> = latest.into_values().collect();
        order.sort_by_key(|&i| self.records[i].occurred_at);

        let mut sent = 0usize;
        let mut delivered_keys: HashSet<Key> = HashSet::new();
        for idx in order {
            let record = self.records[idx].clone();
            if send(record.report.clone()).await {
                delivered_keys.insert(key_of(&record.report));
                sent += 1;
            } else {
                break;
            }
        }

        if !delivered_keys.is_empty() {
            self.records
                .retain(|r| !delivered_keys.contains(&key_of(&r.report)));
            self.rewrite()?;
        }

        Ok(sent)
    }

    fn append_line(&self, record: &ScrobbleRecord) -> Result<(), CacheError> {
        let json = serde_json::to_string(record).expect("ScrobbleRecord always serialises");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| CacheError::Io {
                path: self.path.clone(),
                source,
            })?;
        writeln!(file, "{json}").map_err(|source| CacheError::Io {
            path: self.path.clone(),
            source,
        })
    }

    /// The only non-append write path: a fresh file holding exactly `self.records`, one per line.
    fn rewrite(&self) -> Result<(), CacheError> {
        let mut out = String::new();
        for record in &self.records {
            out.push_str(&serde_json::to_string(record).expect("ScrobbleRecord always serialises"));
            out.push('\n');
        }
        std::fs::write(&self.path, out).map_err(|source| CacheError::Io {
            path: self.path.clone(),
            source,
        })
    }
}

/// A missing file is just an empty buffer. Each line is parsed independently — a corrupt or
/// truncated line (a crash mid-write leaves the final line incomplete) is skipped with a `warn!`,
/// never discarding the rest of the file.
fn load(path: &Path) -> Vec<ScrobbleRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<ScrobbleRecord>(line) {
            Ok(record) => Some(record),
            Err(error) => {
                tracing::warn!(%error, "corrupt scrobble line; skipped");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    fn start(item: &str, session: &str) -> PlaybackReport {
        PlaybackReport::Start {
            item: ItemId::from(item),
            session: PlaySessionId::from(session),
        }
    }

    fn progress(item: &str, session: &str, secs: u64) -> PlaybackReport {
        PlaybackReport::Progress {
            item: ItemId::from(item),
            session: PlaySessionId::from(session),
            position: Duration::from_secs(secs),
            paused: false,
            play_method: loxia_core::model::PlayMethod::DirectStream,
        }
    }

    fn stopped(item: &str, session: &str, secs: u64) -> PlaybackReport {
        PlaybackReport::Stopped {
            item: ItemId::from(item),
            session: PlaySessionId::from(session),
            position: Duration::from_secs(secs),
        }
    }

    fn played(item: &str) -> PlaybackReport {
        PlaybackReport::Played {
            item: ItemId::from(item),
        }
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_second(1_700_000_000 + secs).unwrap()
    }

    fn server() -> ServerId {
        ServerId::from("srv1")
    }

    #[test]
    fn append_writes_one_line_per_record() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        buf.append(start("t1", "s1"), at(0)).unwrap();
        buf.append(progress("t1", "s1", 5), at(1)).unwrap();

        let text = std::fs::read_to_string(dir.path().join("scrobbles.json")).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert_eq!(buf.pending(), 2);
    }

    #[test]
    fn truncated_final_line_loses_only_that_record() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("scrobbles.json");
        let good = serde_json::to_string(&ScrobbleRecord {
            server: server(),
            occurred_at: at(0),
            report: start("t1", "s1"),
        })
        .unwrap();
        let truncated = r#"{"server":"srv1","occurred_at":"2023-11-1"#; // cut mid-write
        std::fs::write(&path, format!("{good}\n{truncated}")).unwrap();

        let buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        assert_eq!(buf.pending(), 1, "only the good line survives");
    }

    #[test]
    fn corrupt_line_is_skipped_and_drain_continues() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("scrobbles.json");
        let good = serde_json::to_string(&ScrobbleRecord {
            server: server(),
            occurred_at: at(0),
            report: played("t1"),
        })
        .unwrap();
        std::fs::write(&path, format!("not json at all\n{good}\n")).unwrap();

        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        assert_eq!(buf.pending(), 1);

        let sent = tokio_test_block_on(buf.drain(|_| async { true }));
        assert_eq!(sent.unwrap(), 1);
    }

    fn tokio_test_block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn drain_collapses_progress_per_session() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        for secs in 0..5 {
            buf.append(progress("t1", "s1", secs), at(secs as i64))
                .unwrap();
        }

        let sent_reports = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = sent_reports.clone();
        let sent = tokio_test_block_on(buf.drain(move |r| {
            let recorder = recorder.clone();
            async move {
                recorder.lock().unwrap().push(r);
                true
            }
        }))
        .unwrap();

        assert_eq!(sent, 1, "five Progress records collapse to one");
        let reports = sent_reports.lock().unwrap();
        assert_eq!(reports.len(), 1);
        assert!(matches!(
            &reports[0],
            PlaybackReport::Progress { position, .. } if *position == Duration::from_secs(4)
        ));
        assert_eq!(buf.pending(), 0);
    }

    #[test]
    fn drain_keeps_start_stopped_and_played() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        buf.append(start("t1", "s1"), at(0)).unwrap();
        buf.append(stopped("t1", "s1", 10), at(1)).unwrap();
        buf.append(played("t1"), at(2)).unwrap();

        let sent = tokio_test_block_on(buf.drain(|_| async { true })).unwrap();
        assert_eq!(sent, 3, "Start, Stopped, and Played are all distinct keys");
    }

    #[test]
    fn drain_replays_chronologically() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        // Appended out of chronological order.
        buf.append(stopped("t1", "s1", 10), at(5)).unwrap();
        buf.append(start("t1", "s1"), at(0)).unwrap();
        buf.append(played("t2"), at(3)).unwrap();

        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = order.clone();
        tokio_test_block_on(buf.drain(move |r| {
            let recorder = recorder.clone();
            async move {
                recorder.lock().unwrap().push(r);
                true
            }
        }))
        .unwrap();

        let order = order.lock().unwrap();
        assert!(matches!(order[0], PlaybackReport::Start { .. }));
        assert!(matches!(order[1], PlaybackReport::Played { .. }));
        assert!(matches!(order[2], PlaybackReport::Stopped { .. }));
    }

    #[test]
    fn drain_stops_at_first_failure_and_retains_remainder() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        buf.append(start("t1", "s1"), at(0)).unwrap();
        buf.append(played("t2"), at(1)).unwrap();
        buf.append(played("t3"), at(2)).unwrap();

        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = calls.clone();
        let sent = tokio_test_block_on(buf.drain(move |_| {
            let counter = counter.clone();
            async move {
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                n == 0 // only the first ("Start") succeeds; the rest fail
            }
        }))
        .unwrap();

        assert_eq!(sent, 1);
        assert_eq!(
            buf.pending(),
            2,
            "the failed record and everything after stays"
        );
    }

    #[test]
    fn drain_deduplicates_by_session_and_kind() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        // Two `Stopped` records for the same (item, session) — e.g. a restart re-issued one.
        buf.append(stopped("t1", "s1", 5), at(0)).unwrap();
        buf.append(stopped("t1", "s1", 10), at(1)).unwrap();

        let sent = tokio_test_block_on(buf.drain(|_| async { true })).unwrap();
        assert_eq!(
            sent, 1,
            "duplicates for the same (item, session, kind) collapse to one"
        );
    }

    #[test]
    fn overflow_drops_oldest_progress_first() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        for i in 0..MAX_RECORDS {
            buf.append(progress("t1", &format!("s{i}"), 0), at(i as i64))
                .unwrap();
        }
        buf.append(played("newest"), at(MAX_RECORDS as i64))
            .unwrap();

        assert_eq!(buf.pending(), MAX_RECORDS);
        // The very first Progress (oldest) must be the one dropped.
        assert!(!buf.records.iter().any(|r| key_of(&r.report)
            == Key {
                item: ItemId::from("t1"),
                session: Some(PlaySessionId::from("s0")),
                kind: Kind::Progress,
            }));
        assert!(buf.records.iter().any(|r| is_played(&r.report)));
    }

    #[test]
    fn overflow_never_drops_played() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        for i in 0..MAX_RECORDS {
            buf.append(played(&format!("t{i}")), at(i as i64)).unwrap();
        }
        buf.append(progress("new", "s", 0), at(MAX_RECORDS as i64))
            .unwrap();

        let played_count = buf.records.iter().filter(|r| is_played(&r.report)).count();
        assert_eq!(played_count, MAX_RECORDS, "every Played record survives");
    }

    #[test]
    fn buffer_survives_restart() {
        let dir = tempdir().unwrap();
        {
            let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
            buf.append(start("t1", "s1"), at(0)).unwrap();
            buf.append(played("t2"), at(1)).unwrap();
        }
        let buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        assert_eq!(buf.pending(), 2);
    }

    #[test]
    fn pending_count_matches_records() {
        let dir = tempdir().unwrap();
        let mut buf = ScrobbleBuffer::open(dir.path(), server()).unwrap();
        assert_eq!(buf.pending(), 0);
        buf.append(start("t1", "s1"), at(0)).unwrap();
        assert_eq!(buf.pending(), 1);
        buf.append(played("t1"), at(1)).unwrap();
        assert_eq!(buf.pending(), 2);
    }
}
