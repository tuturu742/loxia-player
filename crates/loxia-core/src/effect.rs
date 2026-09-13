//! Effect — a request for I/O, emitted by the reducer. Plain data; never a closure or a channel
//! handle (`docs/04-state-and-input.md` §3).

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use crate::config::{Config, QualityProfile, ReplayGainMode};
use crate::model::{
    ImageSize, ItemId, LyricStreamRef, PlaybackReport, PlaylistEntryId, PlaylistId, Track,
};
use crate::state::SessionSnapshot;
use crate::state::nav::{ColumnKind, Tab};
use crate::state::player::SeekTarget;
use crate::state::queue::HistoryEntry;

/// A URL string with `api_key` redacted from `Debug`/`Display` — `Effect` must stay plain,
/// serialisable data, but the access token embedded in a stream URL (`docs/03-emby-api.md` §5)
/// must never leak through a derived `Debug` used for effect-dispatch tracing or replay logs.
/// Deliberately independent of `loxia-emby::stream::StreamUrl` (which wraps `reqwest::Url`):
/// `loxia-core` cannot depend on `reqwest`, so this does plain string surgery on `api_key=...`
/// instead of parsing the URL structurally.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RedactedUrl(String);

impl RedactedUrl {
    pub fn new(url: impl Into<String>) -> Self {
        RedactedUrl(url.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn redact_api_key(url: &str) -> String {
    let Some(pos) = url.find("api_key=") else {
        return url.to_string();
    };
    let value_start = pos + "api_key=".len();
    let value_end = url[value_start..]
        .find('&')
        .map(|i| value_start + i)
        .unwrap_or(url.len());
    format!("{}REDACTED{}", &url[..value_start], &url[value_end..])
}

impl fmt::Debug for RedactedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RedactedUrl({})", redact_api_key(&self.0))
    }
}

impl fmt::Display for RedactedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", redact_api_key(&self.0))
    }
}

/// `11-03`: a plaintext password, carried only as far as `NetEffect::TestServerConnection` (the
/// server-profile editor's "test connection before save" and "save" flows both need to hand a raw
/// password to `workers::network`, which has no `ServerConfig`/`EmbyClient` yet to derive one
/// from) — its whole point, like [`RedactedUrl`]'s, is that a derived `Debug` on `Effect` can never
/// print it, the same "secrets never survive a `Debug`/`Display` used for tracing" rule
/// `ServerConfig`'s own hand-written `Debug` already enforces for `access_token`.
#[derive(Clone, PartialEq, Eq)]
pub struct RedactedSecret(String);

impl RedactedSecret {
    pub fn new(secret: impl Into<String>) -> Self {
        RedactedSecret(secret.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RedactedSecret(<redacted>)")
    }
}

/// A 10-band parametric EQ curve, in dB per band.
pub type EqCurve = [f32; 10];

/// What a `d` press asks to be kept permanently.
///
/// Mirrors `loxia_cache::downloads::DownloadScope`, which is the one that does the work — this
/// crate cannot depend on `loxia-cache`, so the pair is mapped at the worker boundary. `Artist` was
/// missing here (the cache side always had it), and `Track` carried only an id although the
/// downloader needs the whole track to name the file and write its sidecar: both gaps went
/// unnoticed because nothing ever constructed this type at all (`docs/12-decisions.md`).
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadScope {
    Track(Box<crate::model::Track>),
    Album(ItemId),
    Artist(ItemId),
    Playlist(PlaylistId),
}

impl DownloadScope {
    /// The id the download index files this scope under, and the one `RemoveDownload` takes.
    pub fn id(&self) -> ItemId {
        match self {
            DownloadScope::Track(t) => t.id.clone(),
            DownloadScope::Album(id) | DownloadScope::Artist(id) => id.clone(),
            DownloadScope::Playlist(id) => ItemId::from(id.as_str()),
        }
    }
}

/// Desktop notification payload (`loxia`'s `notify-rust` integration).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackChange {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_path: Option<String>,
}

/// MPRIS `org.mpris.MediaPlayer2.Player` metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct MprisMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub position: Duration,
    pub duration: Duration,
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetEffect {
    /// `tab`/`depth` identify which column the reply belongs to — added while implementing
    /// `04-10`'s network worker, which otherwise has no way to know where a `ColumnKind` alone
    /// (ambiguous: the same kind can recur at different depths, e.g. `Tracks` under different
    /// albums) belongs in the stack. The worker echoes both back unchanged into its reply, and
    /// the reducer already discards a reply whose `(tab, depth)` no longer matches the column
    /// that's there now (`03-06`'s `kind` mismatch check does the same job one field over).
    FetchColumn {
        tab: Tab,
        depth: usize,
        kind: ColumnKind,
        page: usize,
    },
    /// `07-02`: the task's own text says `FetchColumn { Favourites }`, but `ColumnKind` has no
    /// such variant — `favorites()` (`loxia-emby`, `02-07`) returns the same three-section
    /// artists/albums/tracks split `search()` does, not a single homogeneous list a `ColumnKind`
    /// column could hold. A dedicated effect, mirroring `07-01`'s own `Search`, is what the actual
    /// data shape needs (`docs/12-decisions.md`).
    FetchFavourites,
    FetchDiscography {
        tab: Tab,
        depth: usize,
        artist: ItemId,
    },
    FetchAlbumTracks {
        tab: Tab,
        depth: usize,
        album: ItemId,
        filter_artist: Option<ItemId>,
    },
    /// Pressing `a`/`A` on an unopened Album row (`06-02`) — distinct from `FetchAlbumTracks`
    /// (which populates a Miller column at `tab`/`depth`): this queues instead, so it carries
    /// neither. Always requests the *complete* tracklist regardless of which key was pressed
    /// (`docs/03-emby-api.md` §4: the `ArtistOnly` filter is applied client-side to the reply, not
    /// baked into the fetch) — the filter decision made at key-press time is remembered in
    /// `AppState::pending_album_queue_fetch`, keyed by `album`, and consumed when the
    /// `DataAction::TracksLoaded` reply lands.
    FetchAlbumTracksForQueue {
        album: ItemId,
    },
    /// Pressing `a`/`A` on an Artist row (`06-02`) — "every track by that artist"
    /// (`discography::artist_tracks`, `docs/03-emby-api.md` §4). Both keys are identical: there is
    /// nothing to filter for an artist's own full track list.
    FetchArtistTracksForQueue {
        artist: ItemId,
    },
    /// `Enter`/`a`/`A` on a Genre row — every track in it. Genres were refused outright with a
    /// toast on the grounds that one can span tens of thousands of tracks, but a user queueing a
    /// genre deliberately is asking for exactly that (`docs/12-decisions.md`). Carries the genre
    /// name, since Emby filters genres by name rather than id.
    FetchGenreTracksForQueue {
        genre: String,
    },
    /// Pressing `a`/`A` on a Folder row (`07-05`) — `recursive` is exactly the same `full_context`
    /// flag `a`/`A` already carry everywhere else (`false`/`true`), reused directly rather than
    /// invented anew: `a` fetches only `folder`'s direct children, `A` fetches its whole subtree.
    FetchFolderTracksForQueue {
        folder: ItemId,
        recursive: bool,
    },
    /// Pressing `a`/`A`/`Enter` on a Playlist row directly in the Playlists tab's own top-level
    /// list (not yet drilled into its tracks) — a real gap found in the field: every other
    /// container row type (`Album`/`Artist`/`Folder`) already queues this way, but `Playlist` had
    /// no equivalent at all, so it silently queued nothing (`docs/12-decisions.md`). Reuses the
    /// exact same endpoint drilling into `ColumnKind::PlaylistTracks` already calls.
    FetchPlaylistTracksForQueue {
        playlist: PlaylistId,
    },
    /// `07-01`: `limit` is always `50` today, mirroring `02-07`'s own test usage — this task's
    /// spec names no other number, so the debounced fetch this reducer issues fixes one rather
    /// than leaving it a magic literal in the network worker.
    Search {
        query: String,
        limit: usize,
    },
    /// `06-08`: `limit` is always `100` today (this task's own spec fixes it, not a user
    /// setting) — carried explicitly rather than hardcoded in the worker so the reducer stays
    /// the single source of truth for what was actually requested.
    InstantMix {
        seed: ItemId,
        limit: usize,
    },
    SetFavorite {
        id: ItemId,
        on: bool,
    },
    /// `10-08`: `overview` empty means "no description was given" — skipped entirely rather than
    /// sent as an empty string, since Emby's own create endpoint silently ignores `Overview` at
    /// creation time anyway (verified live, `02-01`); a non-empty value means the worker follows
    /// up with `loxia_emby::endpoints::playlists::set_overview` once the create reply supplies the
    /// new playlist's id — both requests inside the one spawned task, reported back as a single
    /// `DataAction::PlaylistSaved`/`LoadFailed` regardless of which of the two requests failed.
    PlaylistCreate {
        name: String,
        tracks: Vec<ItemId>,
        overview: String,
    },
    /// `10-08`: `name` is the *display* name of the existing target playlist, threaded through
    /// from the reducer (which has it from the loaded `Tab::Playlists` column) purely so the
    /// worker's own completion event can name it in the `saved to <name>` toast without a second
    /// round-trip just to look it up.
    PlaylistAdd {
        id: PlaylistId,
        name: String,
        tracks: Vec<ItemId>,
    },
    PlaylistRemove {
        id: PlaylistId,
        entries: Vec<PlaylistEntryId>,
    },
    PlaylistDelete {
        id: PlaylistId,
    },
    PlaylistMove {
        id: PlaylistId,
        item: ItemId,
        new_index: usize,
    },
    ReportPlayback(PlaybackReport),
    FetchLyrics {
        track: ItemId,
        stream_ref: LyricStreamRef,
    },
    FetchImage {
        id: ItemId,
        size: ImageSize,
        tag: String,
    },
    /// The connectivity probe (`08-06`, `docs/06-cache-and-offline.md` §6): `GET
    /// /System/Info/Public`, unauthenticated, on a jittered 5s-30s backoff while offline (plus
    /// immediately whenever the user takes an action needing the network) —
    /// `reducer::connectivity::maybe_probe`/`maybe_immediate_probe` emit it,
    /// `workers::network::handle` replies with `Event::System(SystemEvent::ConnectivityChanged(
    /// Reconnecting))` on success and nothing at all on failure. Pre-scoped since `03-03`; never
    /// constructed until this task.
    Reconnect,
    /// `11-03`: the server-profile editor's "Test connection" — authenticates against a
    /// *candidate* server (not necessarily saved yet, and not necessarily the currently active
    /// one) and reports its name/version. `workers::network` builds a fresh, ad-hoc client from
    /// `url`/`headers` rather than using its own already-connected `EmbyClient`, since the whole
    /// point is testing a profile that might not match it at all. Also reused, unchanged, by
    /// "Save" itself — saving a profile always (re-)authenticates so the persisted
    /// `user_id`/`access_token` are always freshly derived, never typed (`docs/12-decisions.md`).
    TestServerConnection {
        url: String,
        headers: BTreeMap<String, String>,
        /// Found the hard way against a real server: Emby's `AuthenticateByName` rejects a
        /// request with no `X-Emby-Authorization` identification header at all, even at login,
        /// before any access token exists — this is what that header's own `DeviceId=` field
        /// needs (`docs/12-decisions.md`). The draft's own `device_id` (stamped immediately when
        /// the profile is created, never generated lazily) is what the caller always has on hand.
        device_id: String,
        username: String,
        password: RedactedSecret,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioEffect {
    Load {
        url: RedactedUrl,
        headers: BTreeMap<String, String>,
        start_at: Duration,
        gain_db: Option<f32>,
    },
    Preload {
        url: RedactedUrl,
        headers: BTreeMap<String, String>,
        gain_db: Option<f32>,
    },
    PlayPause,
    Stop,
    Seek(SeekTarget),
    SetVolume(u8),
    SetMute(bool),
    SetEq(Option<EqCurve>),
    SetReplayGain(ReplayGainMode),
    SetDevice(String),
    EnumerateDevices,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CacheEffect {
    EnsureCached {
        track: Track,
        profile: QualityProfile,
    },
    /// Pull the *upcoming* queue entries into the rolling cache, in the order given, at the same
    /// profile the current track is being played at (`cache.prefetch_next`). Deliberately a single
    /// effect carrying the whole run rather than N `EnsureCached`es: that effect's worker owns one
    /// in-flight fetch slot and cancels whatever is running whenever a *different* key arrives, so
    /// N of them would cancel each other and the current track's own fetch. The worker replaces
    /// any previous prefetch run when a new one lands — the queue has moved on — and never lets it
    /// pre-empt the current track.
    PrefetchAhead {
        tracks: Vec<Track>,
        profile: QualityProfile,
    },
    PinDownload {
        scope: DownloadScope,
    },
    RemoveDownload {
        id: ItemId,
    },
    PersistSession(Box<SessionSnapshot>),
    /// `11-03`: as `PersistSession`, but written to that server's own `session-<id>.json`
    /// (`loxia_cache::session::save_for_server`) rather than the single, always-"current"
    /// `session.json` — "persist the *outgoing* session under its own server id" on a live switch,
    /// so it isn't immediately clobbered by whatever the *new* active server saves next.
    PersistSessionForServer(crate::model::ServerId, Box<SessionSnapshot>),
    AppendHistory(HistoryEntry),
    AppendScrobble(PlaybackReport),
    DrainScrobbles,
    /// `11-03`: "removing any profile offers to delete its cached files and downloads" — deletes
    /// every on-disk tracks/downloads entry scoped to this server id (`docs/06-cache-and-offline.md`
    /// §1's own `tracks/<server_id>/...`/`downloads/<server_id>/...` layout already keys every
    /// path by server, so this is a directory-subtree removal, not a per-item walk through the
    /// rolling cache's or `Downloads`' own in-memory bookkeeping — safe precisely because the
    /// profile being removed is never the *active* one (removal is refused for that one,
    /// `docs/12-decisions.md`), so nothing currently open on this run ever touches that subtree.
    DeleteServerData(crate::model::ServerId),
    /// `11-06`: Settings → Interface → "clear saved session" — deletes the single unsuffixed
    /// `session.json` (never a per-server one; that scope is `DeleteServerData`'s job, tied to
    /// removing a *profile*, not clearing the current one's own saved position) after the reducer
    /// has already emptied `state.queue` itself (`reducer::queue::clear`, reused verbatim from the
    /// server-switch confirmation flow).
    DeleteSession,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SysEffect {
    Notify(TrackChange),
    UpdateMpris(MprisMeta),
    WriteConfig(Box<Config>),
    /// `11-01`: the Settings view's own live-apply for `ui.enable_mouse` — mouse capture is a
    /// terminal-wide escape sequence issued once at startup (`TerminalGuard::enter`), not
    /// something any per-event read of `state.config` can react to on its own, unlike
    /// `desktop_notifications`/`show_lyrics` (checked fresh at the point of use every time)
    /// or `ui.theme` (`state.theme` is rebuilt directly by the reducer, no effect needed).
    SetMouseCapture(bool),
    /// `11-03`: "switching servers... reconnect, and reseed the columns" — emitted once the
    /// confirmed switch has already cleared the queue/history and persisted the outgoing
    /// session's own snapshot. Handled directly in `runtime::run` (like `SetMouseCapture`, ahead
    /// of the generic per-worker `dispatch::dispatch` fan-out): rebuilding the `EmbyClient` and
    /// respawning the network/cache workers needs `&mut Workers`/`&Paths`, neither of which any
    /// worker's own channel has access to (`docs/12-decisions.md`).
    ReconnectServer(crate::model::ServerId),
    /// `11-07`: "[d] Copy diagnostics" — the reducer builds the text (`reducer::settings::
    /// about_diagnostics_text`), redacting the access token and any custom header values; the
    /// runtime writes it out via an OSC 52 terminal escape sequence (`docs/12-decisions.md`),
    /// which needs no new dependency and, unlike an X11/Wayland clipboard crate, works over SSH —
    /// exactly the environment a terminal music client is most likely to be copying a bug report
    /// from.
    CopyToClipboard(String),
    Exit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Net(NetEffect),
    Audio(AudioEffect),
    Cache(Box<CacheEffect>),
    Sys(SysEffect),
}
