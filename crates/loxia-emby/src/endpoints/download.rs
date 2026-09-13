//! Streams a track's audio bytes straight to disk, resumable via HTTP `Range` (`08-03`,
//! `docs/06-cache-and-offline.md` §§4-5) — shared by the rolling cache's background fetch and
//! (`08-04`) permanent downloads.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use reqwest::StatusCode;
use reqwest::header::{CONTENT_RANGE, RANGE};
use tokio::io::AsyncWriteExt;

use crate::client::EmbyClient;
use crate::error::{EmbyError, classify_transport};
use crate::stream::StreamUrl;

/// Fetches `url` to `dest`, resuming from `dest`'s own current size (an HTTP `Range` request) if
/// it already exists — a prior attempt's partial file, or `dest` itself passed in as `<final>.part`
/// mid-download. Checked against `cancel` between chunks, so a caller can stop an in-progress
/// fetch the moment it's no longer relevant (`docs/06-cache-and-offline.md` §4: "a user skipping
/// quickly through an album should not queue up twenty downloads") — a cancellation returns
/// `Ok` with however many bytes had already landed, **not** an error, since stopping on purpose
/// isn't a failure.
///
/// Returns the total size of `dest` once this call returns (whether newly written, resumed, or
/// left short by a cancellation) — never `Err` for anything the caller should treat as anything
/// but "try again later, and the caller's own `.part` file is the resume point."
pub async fn fetch_to_file(
    client: &EmbyClient,
    url: &StreamUrl,
    dest: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<u64, EmbyError> {
    let already_have = tokio::fs::metadata(dest)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let mut request = client.get_url(url.as_url());
    if already_have > 0 {
        request = request.header(RANGE, format!("bytes={already_have}-"));
    }

    let response = request.send().await.map_err(classify_transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(crate::error::classify_status(
            status.as_u16(),
            crate::client::parse_retry_after(response.headers()),
            None,
            String::new(),
        ));
    }

    // The server only actually resumes if it replies 206 with a matching `Content-Range` start —
    // anything else (a plain 200, e.g. a server that ignores `Range` entirely) means the response
    // body is the *whole* file from byte 0, so appending to what's already on disk would corrupt
    // it. Start over instead.
    let resuming = already_have > 0
        && status == StatusCode::PARTIAL_CONTENT
        && response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with(&format!("bytes {already_have}-")));

    let mut file = if resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(dest)
            .await
            .map_err(|source| EmbyError::Io {
                path: dest.to_path_buf(),
                source,
            })?
    } else {
        tokio::fs::File::create(dest)
            .await
            .map_err(|source| EmbyError::Io {
                path: dest.to_path_buf(),
                source,
            })?
    };

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let chunk = chunk.map_err(classify_transport)?;
        file.write_all(&chunk)
            .await
            .map_err(|source| EmbyError::Io {
                path: dest.to_path_buf(),
                source,
            })?;
    }
    file.flush().await.map_err(|source| EmbyError::Io {
        path: dest.to_path_buf(),
        source,
    })?;
    drop(file);

    let final_size = tokio::fs::metadata(dest)
        .await
        .map(|m| m.len())
        .map_err(|source| EmbyError::Io {
            path: dest.to_path_buf(),
            source,
        })?;
    Ok(final_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::config::{QualityProfile, TargetCodec};
    use loxia_core::model::ItemId;
    use std::collections::BTreeMap;
    use tempfile::tempdir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(url: &str) -> loxia_core::config::ServerConfig {
        loxia_core::config::ServerConfig {
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
    async fn fetch_writes_the_whole_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8; 1000]))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let url = StreamUrl::build(
            &client,
            &ItemId::from("item-1"),
            QualityProfile::Direct,
            TargetCodec::Mp3,
        );
        let dir = tempdir().unwrap();
        let dest = dir.path().join("track.part");
        let cancel = Arc::new(AtomicBool::new(false));

        let bytes = fetch_to_file(&client, &url, &dest, &cancel).await.unwrap();
        assert_eq!(bytes, 1000);
        assert_eq!(tokio::fs::metadata(&dest).await.unwrap().len(), 1000);
    }

    #[tokio::test]
    async fn cancellation_leaves_a_partial_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8; 1000]))
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let url = StreamUrl::build(
            &client,
            &ItemId::from("item-1"),
            QualityProfile::Direct,
            TargetCodec::Mp3,
        );
        let dir = tempdir().unwrap();
        let dest = dir.path().join("track.part");
        let cancel = Arc::new(AtomicBool::new(true)); // already cancelled before the first chunk

        let bytes = fetch_to_file(&client, &url, &dest, &cancel).await.unwrap();
        assert!(bytes <= 1000);
        assert!(dest.exists());
    }

    #[tokio::test]
    async fn resumes_with_range_header_against_existing_bytes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/emby/Audio/item-1/stream"))
            .respond_with(move |req: &wiremock::Request| {
                let range = req
                    .headers
                    .get("Range")
                    .map(|v| v.to_str().unwrap().to_string());
                if range.as_deref() == Some("bytes=500-") {
                    ResponseTemplate::new(206)
                        .insert_header("Content-Range", "bytes 500-999/1000")
                        .set_body_bytes(vec![2u8; 500])
                } else {
                    ResponseTemplate::new(200).set_body_bytes(vec![1u8; 1000])
                }
            })
            .mount(&server)
            .await;

        let client = EmbyClient::new(&cfg(&server.uri())).unwrap();
        let url = StreamUrl::build(
            &client,
            &ItemId::from("item-1"),
            QualityProfile::Direct,
            TargetCodec::Mp3,
        );
        let dir = tempdir().unwrap();
        let dest = dir.path().join("track.part");
        tokio::fs::write(&dest, vec![9u8; 500]).await.unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        let bytes = fetch_to_file(&client, &url, &dest, &cancel).await.unwrap();
        assert_eq!(bytes, 1000, "500 already-present + 500 resumed");
    }
}
