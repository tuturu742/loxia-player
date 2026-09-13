//! AuthenticateByName, token validation.

use std::time::Duration;

use reqwest::Url;
use reqwest::header::HeaderMap;
use serde::Deserialize;

use loxia_core::model::UserId;

use crate::client::{append_emby_path, parse_retry_after};
use crate::endpoints::probe;
use crate::error::{EmbyError, classify_status, classify_transport};

#[derive(Debug)]
pub struct AuthResult {
    pub user_id: UserId,
    pub access_token: String,
    pub server_id: String,
}

/// `11-03`: what the server-profile editor's "Test connection" (and, reusing the same flow,
/// "Save") needs — login plus enough of the server's own identity to show `connected to <name>
/// (v<version>)`. `user_id`/`access_token` are carried through so a successful test can also be
/// used to populate the profile being saved, without asking the user to re-enter their password a
/// second time for what is, from their point of view, the same "log in" step.
#[derive(Debug)]
pub struct TestConnectionResult {
    pub user_id: UserId,
    pub access_token: String,
    pub server_name: String,
    pub version: String,
}

/// Authenticates, then reads `System/Info/Public` for the server's own name/version — bundled into
/// one call since the editor never wants one without the other (`docs/12-decisions.md`). A failure
/// at either step is returned as-is; the caller (`workers::network`, `crates/loxia`) turns it into
/// the mapped message this task's own spec asks for via `EmbyError`'s existing `Display`.
pub async fn test_connection(
    base: &Url,
    headers: &HeaderMap,
    device_id: &str,
    user: &str,
    pw: &str,
) -> Result<TestConnectionResult, EmbyError> {
    let auth = authenticate(base, headers, device_id, user, pw).await?;
    let info = probe::info(base, headers).await?;
    Ok(TestConnectionResult {
        user_id: auth.user_id,
        access_token: auth.access_token,
        server_name: info.server_name,
        version: info.version,
    })
}

#[derive(Debug)]
pub struct UserInfo {
    pub id: UserId,
    pub name: String,
}

#[derive(Deserialize)]
struct AuthenticateResponse {
    #[serde(rename = "User")]
    user: AuthenticateUser,
    #[serde(rename = "AccessToken")]
    access_token: String,
    #[serde(rename = "ServerId")]
    server_id: String,
}

#[derive(Deserialize)]
struct AuthenticateUser {
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct UserDto {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
}

/// `POST /emby/Users/AuthenticateByName`. `headers` carries whatever custom headers the server
/// profile defines; `device_id` is stamped separately into the mandatory `X-Emby-Authorization`
/// identification header this function builds itself (`client::build_identification_header`) —
/// Emby rejects the request outright (400, "Value cannot be null. (Parameter 'appName')") without
/// it, even at login, before any token exists. There is no access token yet, so `x-emby-token` (a
/// *separate* header from this one) is deliberately absent.
pub async fn authenticate(
    base: &Url,
    headers: &HeaderMap,
    device_id: &str,
    user: &str,
    pw: &str,
) -> Result<AuthResult, EmbyError> {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("static client configuration is always valid");

    let mut all_headers = crate::client::build_identification_header(device_id)?;
    all_headers.extend(headers.clone());

    let url = append_emby_path(base, "Users/AuthenticateByName");
    let response = http
        .post(url)
        .headers(all_headers)
        .json(&serde_json::json!({ "Username": user, "Pw": pw }))
        .send()
        .await
        .map_err(classify_transport)?;

    let status = response.status();
    let retry_after = parse_retry_after(response.headers());
    if !status.is_success() {
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(status.as_u16(), retry_after, None, message));
    }

    let text = response.text().await.map_err(classify_transport)?;
    let parsed: AuthenticateResponse =
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode {
            endpoint: "Users/AuthenticateByName".to_string(),
            source,
        })?;

    Ok(AuthResult {
        user_id: UserId::from(parsed.user.id),
        access_token: parsed.access_token,
        server_id: parsed.server_id,
    })
}

/// `GET /emby/Users/{user_id}`. A 401/403 maps to [`EmbyError::Unauthorized`] via
/// [`classify_status`], which the UI turns into the re-login flow rather than a crash.
pub async fn validate_token(client: &crate::client::EmbyClient) -> Result<UserInfo, EmbyError> {
    let response = client
        .get(&format!("Users/{}", client.user_id()))
        .send()
        .await
        .map_err(classify_transport)?;

    let status = response.status();
    let retry_after = parse_retry_after(response.headers());
    if !status.is_success() {
        let message = response.text().await.unwrap_or_default();
        return Err(classify_status(status.as_u16(), retry_after, None, message));
    }

    let endpoint = format!("Users/{}", client.user_id());
    let text = response.text().await.map_err(classify_transport)?;
    let dto: UserDto =
        serde_json::from_str(&text).map_err(|source| EmbyError::Decode { endpoint, source })?;

    Ok(UserInfo {
        id: UserId::from(dto.id),
        name: dto.name,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use loxia_core::config::ServerConfig;
    use reqwest::header::{HeaderName, HeaderValue};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::client::EmbyClient;

    fn cfg(url: &str) -> ServerConfig {
        ServerConfig {
            id: "srv".to_string(),
            name: "Test Server".to_string(),
            url: url.to_string(),
            user_id: "user-1".to_string(),
            access_token: "tok-abc".to_string(),
            device_id: "device-abc".to_string(),
            custom_headers: BTreeMap::new(),
            server_id: String::new(),
            fallbacks: Vec::new(),
        }
    }

    #[tokio::test]
    async fn authenticate_returns_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "User": { "Id": "user-42" },
                "AccessToken": "fresh-token",
                "ServerId": "server-9",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let base = Url::parse(&server.uri()).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-custom"),
            HeaderValue::from_static("v"),
        );

        let result = authenticate(&base, &headers, "device-abc", "test", "testTEST1!")
            .await
            .unwrap();
        assert_eq!(result.user_id.as_str(), "user-42");
        assert_eq!(result.access_token, "fresh-token");
        assert_eq!(result.server_id, "server-9");
    }

    /// Found the hard way against a real server: Emby's `AuthenticateByName` rejects a request
    /// with no `X-Emby-Authorization` header at all (400, "Value cannot be null. (Parameter
    /// 'appName')") — `wiremock`'s own matchers never validated header *presence* before, which is
    /// exactly how this shipped unnoticed.
    #[tokio::test]
    async fn authenticate_sends_the_identification_header_even_pre_login() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .and(wiremock::matchers::header_exists("x-emby-authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "User": { "Id": "user-42" },
                "AccessToken": "fresh-token",
                "ServerId": "server-9",
            })))
            .expect(1)
            .mount(&server)
            .await;

        let base = Url::parse(&server.uri()).unwrap();
        authenticate(&base, &HeaderMap::new(), "device-abc", "test", "testTEST1!")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn validate_token_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let err = validate_token(&client).await.unwrap_err();
        assert!(matches!(err, EmbyError::Unauthorized));
    }

    #[tokio::test]
    async fn validate_token_returns_user_info() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Users/user-1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "Id": "user-1", "Name": "Test User" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let info = validate_token(&client).await.unwrap();
        assert_eq!(info.id.as_str(), "user-1");
        assert_eq!(info.name, "Test User");
    }

    #[tokio::test]
    async fn test_connection_succeeds_and_reports_name_and_version() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "User": { "Id": "user-77" },
                "AccessToken": "fresh-token",
                "ServerId": "server-1",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/System/Info/Public"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ServerName": "Home Library",
                "Version": "4.8.0.80",
            })))
            .mount(&server)
            .await;

        let base = Url::parse(&server.uri()).unwrap();
        let result = test_connection(&base, &HeaderMap::new(), "device-abc", "alice", "s3cr3t-pw")
            .await
            .unwrap();
        assert_eq!(result.user_id.as_str(), "user-77");
        assert_eq!(result.access_token, "fresh-token");
        assert_eq!(result.server_name, "Home Library");
        assert_eq!(result.version, "4.8.0.80");
    }

    #[tokio::test]
    async fn test_connection_fails_on_bad_credentials_without_ever_probing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Users/AuthenticateByName"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        // Deliberately not mocked: `test_connection` must return on the auth failure and never
        // reach `System/Info/Public` at all — an unmocked route 404s, which would surface as a
        // *different* error if this function tried it anyway.
        let base = Url::parse(&server.uri()).unwrap();
        let err = test_connection(&base, &HeaderMap::new(), "device-abc", "alice", "wrong-pw")
            .await
            .unwrap_err();
        assert!(matches!(err, EmbyError::Unauthorized));
    }

    #[tokio::test]
    async fn test_connection_maps_unreachable_to_offline() {
        let base = Url::parse("http://127.0.0.1:1").unwrap();
        let err = test_connection(&base, &HeaderMap::new(), "device-abc", "alice", "s3cr3t-pw")
            .await
            .unwrap_err();
        assert!(matches!(err, EmbyError::Offline { .. }));
    }

    #[test]
    fn test_connection_maps_invalid_url() {
        assert!(matches!(
            crate::client::parse_base_url("not a url"),
            Err(EmbyError::InvalidUrl(_))
        ));
    }
}
