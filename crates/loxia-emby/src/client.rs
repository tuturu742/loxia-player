//! EmbyClient: reqwest + base url + auth + custom headers.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use loxia_core::config::ServerConfig;
use loxia_core::model::UserId;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use reqwest::{Client, RequestBuilder, Url};

use crate::error::EmbyError;

const RESERVED_HEADERS: [&str; 4] = [
    "authorization",
    "x-emby-authorization",
    "host",
    "content-length",
];

pub struct EmbyClient {
    http: Client,
    base: Url,
    headers: HeaderMap,
    user_id: UserId,
    device_id: String,
}

impl EmbyClient {
    pub fn new(cfg: &ServerConfig) -> Result<Self, EmbyError> {
        let base = parse_base_url(&cfg.url)?;
        let headers = build_headers(cfg)?;

        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(format!("loxia-player/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("static client configuration is always valid");

        Ok(EmbyClient {
            http,
            base,
            headers,
            user_id: UserId::from(cfg.user_id.clone()),
            device_id: cfg.device_id.clone(),
        })
    }

    /// `{base}/emby/{path}` — `/emby` is appended here, never stored in `base`, so the base stays
    /// exactly what `ServerConfig::url` shows the user in Settings.
    pub fn url(&self, path: &str) -> Url {
        append_emby_path(&self.base, path)
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.http.get(self.url(path)).headers(self.headers.clone())
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.http.post(self.url(path)).headers(self.headers.clone())
    }

    pub fn delete(&self, path: &str) -> RequestBuilder {
        self.http
            .delete(self.url(path))
            .headers(self.headers.clone())
    }

    /// `GET` against an already-fully-built absolute `Url` rather than a relative path — for
    /// `images::fetch`, whose `image_url` builds the full request URL (with query params) up
    /// front so it can be snapshot-tested independently of any network call.
    pub(crate) fn get_url(&self, url: Url) -> RequestBuilder {
        self.http.get(url).headers(self.headers.clone())
    }

    /// `{base}/{path}` — no `/emby` prefix and no auth headers, for the handful of routes Emby
    /// serves outside its API namespace and without authentication (`System/Info/Public`, the
    /// connectivity probe, `08-06`). Sending the auth headers anyway would be harmless, but a
    /// probe's whole point is "can we reach the server at all", not "are we still logged in" —
    /// keeping it header-free means a probe never itself fails for a reason that isn't
    /// connectivity.
    pub(crate) fn get_public(&self, path: &str) -> RequestBuilder {
        let mut url = self.base.clone();
        let base_path = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{base_path}/{}", path.trim_start_matches('/')));
        self.http.get(url)
    }

    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// The raw access token, for `stream::StreamUrl` only — mpv issues its own HTTP request and
    /// does not share this client's header map, so the token must ride in the stream URL's query
    /// string instead. Crate-private: nothing outside `loxia-emby` may read a bare token.
    pub(crate) fn access_token(&self) -> &str {
        self.headers
            .get("x-emby-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    /// `10-12`: `ws::connect_url`'s `deviceId` query parameter — `ws.rs` is part of this same
    /// crate, so crate-private is enough (same visibility as `access_token`, for the same reason).
    pub(crate) fn device_id(&self) -> &str {
        &self.device_id
    }

    /// `10-12`: the same header set every HTTP request already carries (auth + the server
    /// profile's custom headers), applied to the WebSocket upgrade handshake too — "a reverse
    /// proxy will reject the upgrade without them" (this task's own spec). `base()` supplies the
    /// host `ws::connect_url` builds `wss://` from.
    pub(crate) fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub(crate) fn base(&self) -> &Url {
        &self.base
    }
}

/// Prints the base URL and header **names** only — never a header value, so the access token can
/// never reach a log line or panic message through this impl.
impl fmt::Debug for EmbyClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names: Vec<&str> = self.headers.keys().map(HeaderName::as_str).collect();
        names.sort_unstable();
        f.debug_struct("EmbyClient")
            .field("base", &self.base.as_str())
            .field("header_names", &names)
            .finish()
    }
}

pub(crate) fn append_emby_path(base: &Url, path: &str) -> Url {
    let mut url = base.clone();
    let base_path = url.path().trim_end_matches('/').to_string();
    url.set_path(&format!(
        "{base_path}/emby/{}",
        path.trim_start_matches('/')
    ));
    url
}

pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// `11-03`: exposed (was `normalize_base_url`, private) so the server-profile editor's own "test
/// connection before save" flow (`workers::network::test_server_connection`, `crates/loxia`) can
/// validate/normalise a candidate URL the same way `EmbyClient::new` does, without first needing a
/// full `ServerConfig` (there is no `access_token`/`device_id` yet at that point — the whole point
/// of testing before saving).
pub fn parse_base_url(raw: &str) -> Result<Url, EmbyError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(EmbyError::InvalidUrl(raw.to_string()));
    }
    let url = Url::parse(trimmed).map_err(|_| EmbyError::InvalidUrl(raw.to_string()))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(EmbyError::InvalidUrl(raw.to_string())),
    }
}

/// `11-03`: the custom-header half of [`build_headers`], factored out and exposed so the same
/// "test connection before save" flow can build just the reverse-proxy headers a candidate profile
/// needs — `authenticate`/`probe::info` take a raw `HeaderMap`, not a `ServerConfig`, since neither
/// needs (or, for `authenticate`, may even have yet) the Emby auth lines this function omits.
pub fn build_custom_headers(raw: &BTreeMap<String, String>) -> Result<HeaderMap, EmbyError> {
    let mut headers = HeaderMap::new();
    for (name, value) in raw {
        if RESERVED_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            tracing::warn!(header = %name, "skipping reserved custom header — 01-02 should have stripped this already");
            continue;
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| EmbyError::InvalidHeader { name: name.clone() })?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|_| EmbyError::InvalidHeader { name: name.clone() })?;
        headers.insert(header_name, header_value);
    }
    Ok(headers)
}

/// The `X-Emby-Authorization` value every request needs, authenticated or not — Emby's own
/// `AuthenticateByName` (the *login* request itself, before any token exists) rejects a request
/// missing this header with a 400 ("Value cannot be null. (Parameter 'appName')"), found the hard
/// way against a real server: `auth::authenticate` used to send only the caller's own custom
/// headers, with no identification header at all, since nothing before this exercised it against
/// anything but a mock that never validated headers in the first place
/// (`docs/12-decisions.md`). `token` is `""` pre-login (`auth::authenticate`'s own case) or a real
/// access token once one exists (`build_headers`, below, for a saved profile's every other
/// request) — Emby accepts an empty `Token=""` in this same header just fine, which is why only
/// the *separate* `x-emby-token` header (not this one) is conditionally omitted.
fn identification_header_value(device_id: &str, token: &str) -> Result<HeaderValue, EmbyError> {
    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .replace('"', "\\\"");
    let value = format!(
        "MediaBrowser Client=\"loxia-player\", Device=\"{hostname}\", DeviceId=\"{device_id}\", Version=\"{}\", Token=\"{token}\"",
        env!("CARGO_PKG_VERSION"),
    );
    HeaderValue::from_str(&value).map_err(|_| EmbyError::InvalidHeader {
        name: "X-Emby-Authorization".to_string(),
    })
}

/// The identification header alone, as a one-entry `HeaderMap` ready to merge with whatever other
/// headers a request needs — `auth::authenticate`/`auth::test_connection`'s own case, which has no
/// `ServerConfig` yet (a candidate profile that might not be saved) to call [`build_headers`] with.
pub fn build_identification_header(device_id: &str) -> Result<HeaderMap, EmbyError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-emby-authorization"),
        identification_header_value(device_id, "")?,
    );
    Ok(headers)
}

fn build_headers(cfg: &ServerConfig) -> Result<HeaderMap, EmbyError> {
    let mut headers = HeaderMap::new();

    headers.insert(
        HeaderName::from_static("x-emby-authorization"),
        identification_header_value(&cfg.device_id, &cfg.access_token)?,
    );
    headers.insert(
        HeaderName::from_static("x-emby-token"),
        HeaderValue::from_str(&cfg.access_token).map_err(|_| EmbyError::InvalidHeader {
            name: "X-Emby-Token".to_string(),
        })?,
    );

    for (name, value) in build_custom_headers(&cfg.custom_headers)? {
        if let Some(name) = name {
            headers.insert(name, value);
        }
    }

    Ok(headers)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn cfg(url: &str) -> ServerConfig {
        ServerConfig {
            id: "srv".to_string(),
            name: "Test Server".to_string(),
            url: url.to_string(),
            user_id: "user-1".to_string(),
            access_token: "the-secret-token".to_string(),
            device_id: "device-abc".to_string(),
            custom_headers: BTreeMap::new(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    #[test]
    fn url_appends_emby_prefix() {
        let client = EmbyClient::new(&cfg("http://192.168.1.1:8096")).unwrap();
        assert_eq!(
            client.url("Users/1").as_str(),
            "http://192.168.1.1:8096/emby/Users/1"
        );
    }

    #[test]
    fn base_url_normalisation() {
        assert!(
            EmbyClient::new(&cfg("  http://host:8096/  ")).is_ok(),
            "whitespace + trailing slash"
        );
        assert!(
            EmbyClient::new(&cfg("HTTP://host:8096")).is_ok(),
            "mixed-case scheme is normalised, not rejected"
        );
        assert!(
            EmbyClient::new(&cfg("ftp://host:8096")).is_err(),
            "non-http(s) scheme is rejected"
        );
        assert!(
            EmbyClient::new(&cfg("not a url")).is_err(),
            "unparseable input is rejected"
        );
    }

    #[tokio::test]
    async fn sends_expected_auth_headers() {
        let server = MockServer::start().await;
        let mut server_cfg = cfg(&server.uri());
        server_cfg.device_id = "device-xyz".to_string();
        server_cfg.access_token = "tok-123".to_string();
        let client = EmbyClient::new(&server_cfg).unwrap();

        Mock::given(method("GET"))
            .and(path("/emby/probe"))
            .and(header("x-emby-token", "tok-123"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let resp = client.get("probe").send().await.unwrap();
        assert!(resp.status().is_success());

        let auth_header = client
            .headers
            .get("x-emby-authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(auth_header.starts_with("MediaBrowser Client=\"loxia-player\", "));
        assert!(auth_header.contains("DeviceId=\"device-xyz\""));
        assert!(auth_header.contains(&format!("Version=\"{}\"", env!("CARGO_PKG_VERSION"))));
        assert!(auth_header.contains("Token=\"tok-123\""));
    }

    #[tokio::test]
    async fn merges_two_custom_headers() {
        let server = MockServer::start().await;
        let mut server_cfg = cfg(&server.uri());
        server_cfg
            .custom_headers
            .insert("X-Proxy-Key".to_string(), "abc".to_string());
        server_cfg
            .custom_headers
            .insert("X-Region".to_string(), "eu".to_string());
        let client = EmbyClient::new(&server_cfg).unwrap();

        Mock::given(method("GET"))
            .and(path("/emby/probe"))
            .and(header("x-proxy-key", "abc"))
            .and(header("x-region", "eu"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let resp = client.get("probe").send().await.unwrap();
        assert!(resp.status().is_success());
    }

    #[test]
    fn skips_reserved_custom_header() {
        let mut server_cfg = cfg("http://host:8096");
        server_cfg
            .custom_headers
            .insert("Authorization".to_string(), "Bearer evil".to_string());
        let client = EmbyClient::new(&server_cfg).unwrap();

        // The reserved header must not have clobbered the real auth header.
        let auth_header = client
            .headers
            .get("x-emby-authorization")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(auth_header.contains("Token=\"the-secret-token\""));
        assert!(client.headers.get("authorization").is_none());
    }

    #[test]
    fn invalid_header_value_is_an_error() {
        let mut server_cfg = cfg("http://host:8096");
        server_cfg
            .custom_headers
            .insert("X-Bad".to_string(), "value\r\nwith-crlf".to_string());
        let err = EmbyClient::new(&server_cfg).unwrap_err();
        assert!(matches!(err, EmbyError::InvalidHeader { name } if name == "X-Bad"));
    }

    #[test]
    fn client_debug_redacts_token() {
        let client = EmbyClient::new(&cfg("http://host:8096")).unwrap();
        let debug = format!("{client:?}");
        assert!(!debug.contains("the-secret-token"));
        assert!(debug.contains("header_names"));
    }

    #[test]
    fn user_id_accessor() {
        let client = EmbyClient::new(&cfg("http://host:8096")).unwrap();
        assert_eq!(client.user_id().as_str(), "user-1");
    }
}
