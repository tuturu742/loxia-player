//! Instant mix generation.

use loxia_core::model::{ItemId, Track};

use crate::client::EmbyClient;
use crate::error::EmbyError;

use super::fetch_typed;

/// `GET /Items/{seed}/InstantMix?UserId={uid}&Limit={limit}`. Works for any item type. An empty
/// result is `Ok(vec![])`, not an error — some libraries genuinely cannot generate a mix, and the
/// UI shows an explanatory toast rather than an error one.
pub async fn instant_mix(
    client: &EmbyClient,
    seed: &ItemId,
    limit: usize,
) -> Result<Vec<Track>, EmbyError> {
    let path = format!("Items/{seed}/InstantMix");
    let pairs = vec![
        ("UserId".to_string(), client.user_id().to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    fetch_typed::<Track>(client, &path, &pairs).await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use loxia_core::config::ServerConfig;

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

    fn load_fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn instant_mix_returns_tracks() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/InstantMix"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(load_fixture("instant_mix.json")),
            )
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let tracks = instant_mix(&client, &ItemId::from("t1"), 100)
            .await
            .unwrap();
        assert!(!tracks.is_empty());
    }

    #[tokio::test]
    async fn instant_mix_empty_is_ok() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Items/t1/InstantMix"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "TotalRecordCount": 0,
                "StartIndex": 0,
                "Items": [],
            })))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let tracks = instant_mix(&client, &ItemId::from("t1"), 100)
            .await
            .unwrap();
        assert!(tracks.is_empty());
    }
}
