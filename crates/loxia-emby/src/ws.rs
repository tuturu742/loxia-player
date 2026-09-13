//! WebSocket session: connect-URL/handshake building, inbound message parsing, and reconnect
//! backoff (`10-12`, `docs/03-emby-api.md` §10). Everything here is pure/connection-agnostic;
//! `crates/loxia-player/src/workers/network.rs` owns the actual `tokio-tungstenite` connection, its
//! reconnect loop, and the keepalive timer.

use std::time::Duration;

use loxia_core::action::PlayerAction;
use loxia_core::model::ItemId;
use loxia_core::state::player::SeekTarget;
use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;

use crate::client::EmbyClient;
use crate::dto::ticks::ticks_to_duration;

/// `wss://{host}/embywebsocket?api_key={token}&deviceId={device_id}` — string surgery on
/// `client`'s own base URL (`EmbyClient::base`), the same approach `client::append_emby_path`
/// already uses, rather than building a URL from scratch: a reverse-proxy path prefix in the
/// user's configured server URL survives unchanged. Deliberately **not** under `/emby` (unlike
/// every REST endpoint) — `docs/03-emby-api.md` §10's own URL has no `/emby` segment.
pub fn connect_url(client: &EmbyClient) -> Url {
    let mut url = client.base().clone();
    let ws_scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(ws_scheme).expect(
        "http(s) and ws(s) are both \"special\" schemes to the url crate; this never fails",
    );
    let base_path = url.path().trim_end_matches('/').to_string();
    url.set_path(&format!("{base_path}/embywebsocket"));
    url.query_pairs_mut()
        .append_pair("api_key", client.access_token())
        .append_pair("deviceId", client.device_id());
    url
}

/// The actual handshake request: [`connect_url`] plus every header this client's own HTTP
/// requests already carry (auth + the server profile's custom headers) — "a reverse proxy will
/// reject the upgrade without them" (this task's own spec). Built from `url.as_str()`, not the
/// `Url` object itself, so this never depends on `tungstenite`'s own optional `url` feature.
pub fn handshake_request(client: &EmbyClient) -> Result<Request, WsError> {
    let mut request = connect_url(client).as_str().into_client_request()?;
    for (name, value) in client.headers().iter() {
        request.headers_mut().insert(name.clone(), value.clone());
    }
    Ok(request)
}

/// One parsed inbound message. A `Vec` (not `Option`) since `UserDataChanged` carries a whole
/// list, and forwards zero, one, or several events.
#[derive(Debug, Clone, PartialEq)]
pub enum WsEvent {
    Player(PlayerAction),
    /// The plain message text — `workers::network` wraps it into `SystemEvent::Toast { message,
    /// level: Info }`; `AppState::toast` (the reducer, `10-13`) is the sole assigner of a real
    /// `Toast`'s own `id`/`created_at`, so nothing here or in `workers::network` needs to touch
    /// either.
    Toast(String),
    LibraryChanged,
    UserDataChanged {
        id: ItemId,
        is_favorite: bool,
        play_count: u32,
    },
    /// The server's requested keepalive interval.
    ForceKeepAlive(Duration),
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "MessageType")]
    message_type: String,
    #[serde(rename = "Data")]
    data: Option<Value>,
}

/// Defensive: a malformed payload (bad JSON, or a recognized `MessageType` whose `Data` doesn't
/// match its expected shape) returns an empty `Vec` rather than an `Err` — "a malformed payload
/// does not drop the connection" (this task's own spec). An unrecognized `MessageType` is logged
/// at `debug` and also produces nothing — the full set this task's own table names is `Playstate`/
/// `GeneralCommand`/`LibraryChanged`/`UserDataChanged`/`ForceKeepAlive`; every other real Emby
/// message type (`Sessions`, `ServerShuttingDown`, `RestartRequired`, ...) falls into this same
/// "ignored" arm, not a separate one, since none of them are in that table.
pub fn parse_message(text: &str) -> Vec<WsEvent> {
    let envelope: Envelope = match serde_json::from_str(text) {
        Ok(envelope) => envelope,
        Err(error) => {
            tracing::debug!(%error, "malformed websocket payload");
            return Vec::new();
        }
    };
    match envelope.message_type.as_str() {
        "Playstate" => parse_playstate(envelope.data).into_iter().collect(),
        "GeneralCommand" => parse_general_command(envelope.data).into_iter().collect(),
        "LibraryChanged" => vec![WsEvent::LibraryChanged],
        "UserDataChanged" => parse_user_data_changed(envelope.data),
        "ForceKeepAlive" => parse_force_keep_alive(envelope.data).into_iter().collect(),
        other => {
            tracing::debug!(message_type = other, "unknown websocket message type");
            Vec::new()
        }
    }
}

#[derive(Deserialize)]
struct PlaystateData {
    #[serde(rename = "Command")]
    command: String,
    #[serde(rename = "SeekPositionTicks", default)]
    seek_position_ticks: i64,
}

/// `docs/03-emby-api.md` §10's own table simplifies Emby's real `Pause`/`Unpause` pair to a
/// single "`PlayPause`" row, matching this app's own `PlayerAction::PlayPause` toggle (there is
/// no separate `Play`/`Pause` action to pick between) — both real commands, plus a literal
/// `"PlayPause"` some clients may send, all map here.
fn parse_playstate(data: Option<Value>) -> Option<WsEvent> {
    let data: PlaystateData = serde_json::from_value(data?).ok()?;
    let action = match data.command.as_str() {
        "Pause" | "Unpause" | "PlayPause" => PlayerAction::PlayPause,
        "NextTrack" => PlayerAction::Next,
        "PreviousTrack" => PlayerAction::Prev,
        "Stop" => PlayerAction::Stop,
        "Seek" => PlayerAction::Seek(SeekTarget::Absolute(ticks_to_duration(
            data.seek_position_ticks,
        ))),
        _ => return None,
    };
    Some(WsEvent::Player(action))
}

#[derive(Deserialize, Default)]
struct GeneralCommandArguments {
    #[serde(rename = "Volume", default)]
    volume: Option<u8>,
    #[serde(rename = "Header", default)]
    header: Option<String>,
    #[serde(rename = "Text", default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeneralCommandData {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Arguments", default)]
    arguments: GeneralCommandArguments,
}

fn parse_general_command(data: Option<Value>) -> Option<WsEvent> {
    let data: GeneralCommandData = serde_json::from_value(data?).ok()?;
    match data.name.as_str() {
        "SetVolume" => Some(WsEvent::Player(PlayerAction::SetVolume(
            data.arguments.volume?,
        ))),
        // `PlayerAction::SetMute` (`10-12`), not `ToggleMute` — `Mute`/`Unmute` are each an
        // absolute request, and toggling could accidentally invert an already-correct state.
        "Mute" => Some(WsEvent::Player(PlayerAction::SetMute(true))),
        "Unmute" => Some(WsEvent::Player(PlayerAction::SetMute(false))),
        "DisplayMessage" => {
            let text = data.arguments.text.unwrap_or_default();
            let message = match data.arguments.header {
                Some(header) if !header.is_empty() => format!("{header}: {text}"),
                _ => text,
            };
            Some(WsEvent::Toast(message))
        }
        _ => None,
    }
}

#[derive(Deserialize)]
struct UserItemData {
    #[serde(rename = "ItemId")]
    item_id: String,
    #[serde(rename = "IsFavorite", default)]
    is_favorite: bool,
    #[serde(rename = "PlayCount", default)]
    play_count: u32,
}

#[derive(Deserialize)]
struct UserDataChangedData {
    #[serde(rename = "UserDataList", default)]
    user_data_list: Vec<UserItemData>,
}

fn parse_user_data_changed(data: Option<Value>) -> Vec<WsEvent> {
    let Some(data) = data else {
        return Vec::new();
    };
    let Ok(data) = serde_json::from_value::<UserDataChangedData>(data) else {
        return Vec::new();
    };
    data.user_data_list
        .into_iter()
        .map(|entry| WsEvent::UserDataChanged {
            id: ItemId::from(entry.item_id),
            is_favorite: entry.is_favorite,
            play_count: entry.play_count,
        })
        .collect()
}

/// `{"MessageType":"ForceKeepAlive","Data":60}` — `Data` is a bare integer, the server's
/// requested keepalive interval in seconds.
fn parse_force_keep_alive(data: Option<Value>) -> Option<WsEvent> {
    let seconds: u64 = serde_json::from_value(data?).ok()?;
    Some(WsEvent::ForceKeepAlive(Duration::from_secs(seconds)))
}

/// This app's own default outbound cadence — "keepalive every 30s" (this task's own spec).
/// `workers::network`'s session loop adopts a server-requested `ForceKeepAlive` interval instead,
/// once one arrives.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// The outbound `KeepAlive` message this app sends every [`KEEPALIVE_INTERVAL`] (or whatever
/// `ForceKeepAlive` last requested). No `Data` field — Emby's own client implementations never
/// send one on this particular message type.
pub fn keepalive_message() -> String {
    r#"{"MessageType":"KeepAlive"}"#.to_string()
}

/// Jittered exponential backoff, 1s -> 30s, doubling per attempt (`attempt` is 0-indexed and
/// reset to `0` by the caller on every successful connection — this function is otherwise
/// stateless). `jitter_frac` (expected `0.0..=1.0`, clamped defensively) is injected rather than
/// read from a live RNG here, the same "seeded, so tests are deterministic" precedent
/// `queue::shuffle` already established in `loxia-core`; real callers pass `rand::random::<f64>()`.
/// Result is always in `[0.5 * base, base]` seconds, where `base` is `2^attempt` capped at 30 —
/// bounded away from both a thundering-herd `0` and an unbounded delay.
pub fn reconnect_backoff(attempt: u32, jitter_frac: f64) -> Duration {
    let base_secs = 1u64.checked_shl(attempt.min(5)).unwrap_or(32).min(30);
    let jitter_frac = jitter_frac.clamp(0.0, 1.0);
    Duration::from_secs_f64(base_secs as f64 * (0.5 + 0.5 * jitter_frac))
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::ServerConfig;

    fn test_config(url: &str) -> ServerConfig {
        ServerConfig {
            id: "srv1".to_string(),
            name: "Test".to_string(),
            url: url.to_string(),
            user_id: "u1".to_string(),
            access_token: "tok123".to_string(),
            device_id: "device-abc".to_string(),
            custom_headers: [("X-Proxy-Auth".to_string(), "secret-header".to_string())]
                .into_iter()
                .collect(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    #[test]
    fn connect_url_rewrites_scheme_and_appends_path_and_query() {
        let client = EmbyClient::new(&test_config("https://media.example.com:8096/proxy")).unwrap();
        let url = connect_url(&client);
        assert_eq!(url.scheme(), "wss");
        assert_eq!(url.host_str(), Some("media.example.com"));
        assert_eq!(url.port(), Some(8096));
        assert_eq!(url.path(), "/proxy/embywebsocket");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs.get("api_key"), Some(&"tok123".to_string()));
        assert_eq!(pairs.get("deviceId"), Some(&"device-abc".to_string()));
    }

    #[test]
    fn connect_url_uses_plain_ws_for_plain_http() {
        let client = EmbyClient::new(&test_config("http://192.168.1.10:8096")).unwrap();
        assert_eq!(connect_url(&client).scheme(), "ws");
    }

    /// `10-12`: `handshake_includes_custom_headers`.
    #[test]
    fn handshake_includes_custom_headers() {
        let client = EmbyClient::new(&test_config("https://media.example.com")).unwrap();
        let request = handshake_request(&client).unwrap();
        assert_eq!(
            request.headers().get("X-Proxy-Auth").unwrap(),
            "secret-header"
        );
        assert!(request.headers().contains_key("x-emby-authorization"));
    }

    /// `10-12`: `playstate_message_mapping` — table test over all five.
    #[test]
    fn playstate_message_mapping() {
        let cases = [
            (
                r#"{"MessageType":"Playstate","Data":{"Command":"Pause"}}"#,
                vec![WsEvent::Player(PlayerAction::PlayPause)],
            ),
            (
                r#"{"MessageType":"Playstate","Data":{"Command":"Unpause"}}"#,
                vec![WsEvent::Player(PlayerAction::PlayPause)],
            ),
            (
                r#"{"MessageType":"Playstate","Data":{"Command":"NextTrack"}}"#,
                vec![WsEvent::Player(PlayerAction::Next)],
            ),
            (
                r#"{"MessageType":"Playstate","Data":{"Command":"PreviousTrack"}}"#,
                vec![WsEvent::Player(PlayerAction::Prev)],
            ),
            (
                r#"{"MessageType":"Playstate","Data":{"Command":"Stop"}}"#,
                vec![WsEvent::Player(PlayerAction::Stop)],
            ),
            (
                r#"{"MessageType":"Playstate","Data":{"Command":"Seek","SeekPositionTicks":50000000}}"#,
                vec![WsEvent::Player(PlayerAction::Seek(SeekTarget::Absolute(
                    Duration::from_secs(5),
                )))],
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_message(text), expected, "{text}");
        }
    }

    /// `10-12`: `general_command_mapping`.
    #[test]
    fn general_command_mapping() {
        let cases = [
            (
                r#"{"MessageType":"GeneralCommand","Data":{"Name":"SetVolume","Arguments":{"Volume":42}}}"#,
                vec![WsEvent::Player(PlayerAction::SetVolume(42))],
            ),
            (
                r#"{"MessageType":"GeneralCommand","Data":{"Name":"Mute"}}"#,
                vec![WsEvent::Player(PlayerAction::SetMute(true))],
            ),
            (
                r#"{"MessageType":"GeneralCommand","Data":{"Name":"Unmute"}}"#,
                vec![WsEvent::Player(PlayerAction::SetMute(false))],
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_message(text), expected, "{text}");
        }
    }

    /// `10-12`: `display_message_becomes_toast`.
    #[test]
    fn display_message_becomes_toast() {
        let text = r#"{"MessageType":"GeneralCommand","Data":{"Name":"DisplayMessage","Arguments":{"Header":"Server","Text":"Restarting soon"}}}"#;
        assert_eq!(
            parse_message(text),
            vec![WsEvent::Toast("Server: Restarting soon".to_string())]
        );

        let no_header = r#"{"MessageType":"GeneralCommand","Data":{"Name":"DisplayMessage","Arguments":{"Text":"Hello"}}}"#;
        assert_eq!(
            parse_message(no_header),
            vec![WsEvent::Toast("Hello".to_string())]
        );
    }

    #[test]
    fn library_changed_produces_one_event() {
        let text = r#"{"MessageType":"LibraryChanged","Data":{"ItemsUpdated":["abc"]}}"#;
        assert_eq!(parse_message(text), vec![WsEvent::LibraryChanged]);
    }

    #[test]
    fn user_data_changed_produces_one_event_per_entry() {
        let text = r#"{
            "MessageType": "UserDataChanged",
            "Data": {
                "UserDataList": [
                    {"ItemId": "item-1", "IsFavorite": true, "PlayCount": 3},
                    {"ItemId": "item-2", "IsFavorite": false, "PlayCount": 0}
                ]
            }
        }"#;
        assert_eq!(
            parse_message(text),
            vec![
                WsEvent::UserDataChanged {
                    id: ItemId::from("item-1"),
                    is_favorite: true,
                    play_count: 3,
                },
                WsEvent::UserDataChanged {
                    id: ItemId::from("item-2"),
                    is_favorite: false,
                    play_count: 0,
                },
            ]
        );
    }

    #[test]
    fn force_keep_alive_carries_the_requested_interval() {
        let text = r#"{"MessageType":"ForceKeepAlive","Data":60}"#;
        assert_eq!(
            parse_message(text),
            vec![WsEvent::ForceKeepAlive(Duration::from_secs(60))]
        );
    }

    /// `10-12`: `unknown_message_type_is_ignored`.
    #[test]
    fn unknown_message_type_is_ignored() {
        let text = r#"{"MessageType":"ServerShuttingDown","Data":{}}"#;
        assert_eq!(parse_message(text), Vec::new());
    }

    /// `10-12`: `malformed_payload_does_not_drop_connection` — `parse_message` returning an empty
    /// `Vec` rather than panicking or propagating an `Err` is what "does not drop the connection"
    /// means at this pure-function layer; the session loop simply has nothing to forward.
    #[test]
    fn malformed_payload_does_not_drop_connection() {
        assert_eq!(parse_message("not json at all"), Vec::new());
        assert_eq!(parse_message(r#"{"MessageType": 42}"#), Vec::new());
        assert_eq!(
            parse_message(r#"{"MessageType":"Playstate","Data":"not an object"}"#),
            Vec::new()
        );
        assert_eq!(
            parse_message(r#"{"MessageType":"Playstate"}"#),
            Vec::new(),
            "missing Data entirely"
        );
    }

    /// `10-12`: `reconnect_backoff_is_jittered_and_capped`.
    #[test]
    fn reconnect_backoff_is_jittered_and_capped() {
        assert_eq!(reconnect_backoff(0, 1.0), Duration::from_secs(1));
        assert_eq!(reconnect_backoff(0, 0.0), Duration::from_millis(500));
        // Doubling: attempt 4 -> base 16s.
        assert_eq!(reconnect_backoff(4, 1.0), Duration::from_secs(16));
        // Capped at 30s regardless of how large `attempt` grows.
        assert_eq!(reconnect_backoff(5, 1.0), Duration::from_secs(30));
        assert_eq!(reconnect_backoff(50, 1.0), Duration::from_secs(30));
        assert!(reconnect_backoff(50, 0.0) <= Duration::from_secs(30));
        // Jittered: two different fractions at the same attempt must differ.
        assert_ne!(reconnect_backoff(3, 0.0), reconnect_backoff(3, 1.0));
    }
}
