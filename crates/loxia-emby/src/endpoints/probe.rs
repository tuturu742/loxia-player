//! Connectivity probe (`08-06`, `docs/06-cache-and-offline.md` §6): `GET /System/Info/Public`,
//! unauthenticated, checked only for "did a response come back at all" — never decoded, since the
//! probe's only job is proving the server is reachable, not reading anything from it.
//!
//! `11-03`: [`info`] is the second, decoding caller of the same endpoint — the server-profile
//! editor's "Test connection" wants `connected to <name> (v<version>)`, which [`probe`]'s own
//! discard-the-body contract can't give it. Kept as a separate function rather than changing
//! `probe`'s own return type: every existing caller of `probe` (the connectivity backoff loop)
//! only ever cared about reachability and would gain nothing from a payload it already ignores.

use reqwest::Url;
use reqwest::header::HeaderMap;
use serde::Deserialize;

use crate::client::EmbyClient;
use crate::error::{EmbyError, classify_status, classify_transport};

/// `Ok(())` if the server answered at all (any HTTP status — even a `4xx`/`5xx` response proves
/// the network path itself works); `Err` only for a genuine transport failure or a status
/// `classify_status` maps to something worse than "server had a bad moment."
pub async fn probe(client: &EmbyClient) -> Result<(), EmbyError> {
    let response = client
        .get_public("System/Info/Public")
        .send()
        .await
        .map_err(classify_transport)?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    Err(classify_status(status.as_u16(), None, None, String::new()))
}

pub struct SystemInfoPublic {
    pub server_name: String,
    pub version: String,
    /// The Emby installation's own GUID — the same value whichever address you reach it on.
    ///
    /// This is what makes "the same server" a decidable question: a LAN `http://` profile and an
    /// external `https://` one are two *endpoints*, not two servers, and everything stored per
    /// server (cache, downloads, session, scrobbles) should be shared between them rather than
    /// duplicated (`docs/12-decisions.md`). Empty if an older server omits it.
    pub id: String,
}

#[derive(Deserialize)]
struct SystemInfoPublicResponse {
    #[serde(rename = "ServerName")]
    server_name: String,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Id", default)]
    id: String,
}

/// The server's own identity, via an already-built client (`info` takes a raw base/headers because
/// the profile editor tests a *candidate* profile that has no client yet).
pub async fn identity(client: &EmbyClient) -> Result<SystemInfoPublic, EmbyError> {
    let response = client
        .get_public("System/Info/Public")
        .send()
        .await
        .map_err(classify_transport)?;
    let status = response.status();
    if !status.is_success() {
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(status.as_u16(), None, None, message));
    }
    let text = response.text().await.map_err(classify_transport)?;
    let parsed: SystemInfoPublicResponse =
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
            endpoint: "System/Info/Public".to_string(),
            source,
        })?;
    Ok(SystemInfoPublic {
        server_name: parsed.server_name,
        version: parsed.version,
        id: parsed.id,
    })
}

/// `GET /System/Info/Public`, decoded — takes a raw `base`/`headers` rather than an `EmbyClient`
/// (like `auth::authenticate`) since the server-profile editor's "test before save" flow has
/// neither a persisted profile nor an access token yet; `headers` carries only whatever custom
/// (e.g. reverse-proxy) headers the candidate profile defines.
pub async fn info(base: &Url, headers: &HeaderMap) -> Result<SystemInfoPublic, EmbyError> {
    let mut url = base.clone();
    let base_path = url.path().trim_end_matches('/').to_string();
    url.set_path(&format!("{base_path}/System/Info/Public"));

    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("static client configuration is always valid");

    let response = http
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(classify_transport)?;

    let status = response.status();
    if !status.is_success() {
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(status.as_u16(), None, None, message));
    }

    let text = response.text().await.map_err(classify_transport)?;
    let parsed: SystemInfoPublicResponse =
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
            endpoint: "System/Info/Public".to_string(),
            source,
        })?;

    Ok(SystemInfoPublic {
        server_name: parsed.server_name,
        version: parsed.version,
        id: parsed.id,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use loxia_core::config::ServerConfig;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn cfg(url: &str) -> ServerConfig {
        ServerConfig {
            id: "srv".to_string(),
            name: "Test".to_string(),
            url: url.to_string(),
            user_id: "user-1".to_string(),
            access_token: "tok".to_string(),
            device_id: "dev".to_string(),
            custom_headers: BTreeMap::new(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    #[tokio::test]
    async fn probe_succeeds_against_system_info_public() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        assert!(probe(&client).await.is_ok());
    }

    #[tokio::test]
    async fn probe_does_not_use_the_emby_path_prefix() {
        let server = MockServer::start().await;
        // Only the un-prefixed route is mocked; if `probe` mistakenly requested
        // `/emby/System/Info/Public`, wiremock would 404 it and this test would fail.
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        assert!(probe(&client).await.is_ok());
    }

    #[tokio::test]
    async fn probe_fails_when_unreachable() {
        // A port nothing listens on — a genuine connection failure, not a mocked 4xx/5xx.
        let client = EmbyClient::new(&cfg("http://127.0.0.1:1")).unwrap();
        assert!(matches!(
            probe(&client).await,
            Err(EmbyError::Offline { .. })
        ));
    }

    #[tokio::test]
    async fn info_decodes_name_and_version() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ServerName": "Home Library",
                "Version": "4.8.0.80",
            })))
            .mount(&server)
            .await;

        let base = Url::parse(&server.uri()).unwrap();
        let info = info(&base, &HeaderMap::new()).await.unwrap();
        assert_eq!(info.server_name, "Home Library");
        assert_eq!(info.version, "4.8.0.80");
    }

    #[tokio::test]
    async fn info_does_not_use_the_emby_path_prefix() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ServerName": "Home Library",
                "Version": "4.8.0.80",
            })))
            .mount(&server)
            .await;

        let base = Url::parse(&server.uri()).unwrap();
        assert!(info(&base, &HeaderMap::new()).await.is_ok());
    }

    #[tokio::test]
    async fn info_maps_unauthorized_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let base = Url::parse(&server.uri()).unwrap();
        assert!(matches!(
            info(&base, &HeaderMap::new()).await,
            Err(EmbyError::Unauthorized)
        ));
    }
}
