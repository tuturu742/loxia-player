//! EmbyError classification.
//!
//! Two classification points, matching where a request can actually fail: `classify_transport`
//! for a failed `reqwest::Error` (never reached the server), and `classify_status` for a response
//! that came back with a non-success HTTP status. Callers compose these; see `retry.rs` for the
//! policy built on top.

use std::time::Duration;

use loxia_core::model::ItemId;

#[derive(Debug, thiserror::Error)]
pub enum EmbyError {
    #[error("could not reach the server.")]
    Offline { source: reqwest::Error },

    #[error("not authorised — the server rejected the credentials.")]
    Unauthorized,

    #[error("item not found.")]
    NotFound { item: Option<ItemId> },

    #[error("the server is temporarily unavailable.")]
    Transient {
        status: u16,
        retry_after: Option<Duration>,
    },

    #[error("could not understand the server's response.")]
    Decode {
        endpoint: String,
        source: serde_json::Error,
    },

    #[error("invalid server url.")]
    InvalidUrl(String),

    #[error("invalid custom header.")]
    InvalidHeader { name: String },

    /// Found while debugging a real "Test connection" failure: the status code costs nothing to
    /// show (it's never a secret) and is often immediately actionable — a wrong port hitting some
    /// other service, a reverse proxy's own error page, a webroot path prefix Emby needs that
    /// wasn't in the URL — where the bare phrase "the server reported an error" gives the user
    /// nothing to act on. `message` (the response body — HTML from a proxy, plain text, or a JSON
    /// error) still isn't shown on screen, since it can be arbitrarily large or ugly; it goes to
    /// the log file instead (`workers::network::test_server_connection`'s own `tracing::warn!`).
    #[error("the server reported an error (HTTP {status}).")]
    Server { status: u16, message: String },

    /// `08-03`: a *local* filesystem failure while streaming `endpoints::download::fetch_to_file`
    /// to disk — distinct from every other variant here, which are all about the HTTP round trip
    /// itself succeeding or failing, not what happens to the bytes once they arrive.
    #[error("could not write the downloaded file: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Classifies a transport-level failure — the request never got a response at all.
pub fn classify_transport(err: reqwest::Error) -> EmbyError {
    if err.is_connect() || err.is_timeout() {
        EmbyError::Offline { source: err }
    } else if err.is_decode() {
        // No endpoint/body context at this layer; endpoint-aware decode errors are constructed
        // directly by the caller instead, which is why this arm still needs somewhere to go.
        EmbyError::Server {
            status: 0,
            message: err.to_string(),
        }
    } else {
        EmbyError::Offline { source: err }
    }
}

/// Classifies a response that came back with a non-success HTTP status. `item` is supplied by
/// the caller for the `NotFound` case, since the raw status alone doesn't know which item was
/// being requested.
pub fn classify_status(
    status: u16,
    retry_after: Option<Duration>,
    item: Option<ItemId>,
    message: String,
) -> EmbyError {
    match status {
        401 | 403 => EmbyError::Unauthorized,
        404 => EmbyError::NotFound { item },
        429 => EmbyError::Transient {
            status,
            retry_after,
        },
        500..=599 => EmbyError::Transient {
            status,
            retry_after,
        },
        _ => EmbyError::Server { status, message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_table() {
        assert!(matches!(
            classify_status(401, None, None, String::new()),
            EmbyError::Unauthorized
        ));
        assert!(matches!(
            classify_status(403, None, None, String::new()),
            EmbyError::Unauthorized
        ));
        assert!(matches!(
            classify_status(404, None, Some(ItemId::from("x")), String::new()),
            EmbyError::NotFound { item: Some(_) }
        ));
        assert!(matches!(
            classify_status(429, Some(Duration::from_secs(1)), None, String::new()),
            EmbyError::Transient { status: 429, .. }
        ));
        assert!(matches!(
            classify_status(500, None, None, String::new()),
            EmbyError::Transient { status: 500, .. }
        ));
        assert!(matches!(
            classify_status(599, None, None, String::new()),
            EmbyError::Transient { status: 599, .. }
        ));
        assert!(matches!(
            classify_status(418, None, None, "teapot".to_string()),
            EmbyError::Server { status: 418, .. }
        ));
    }

    /// Found while debugging a real "the server reported an error" report with nothing else to
    /// go on: the bare phrase alone gives the user no way to tell a wrong port from a reverse
    /// proxy's own error page from a missing webroot path — the status code is never a secret and
    /// costs nothing to show.
    #[test]
    fn server_error_display_includes_the_status_code() {
        let err = EmbyError::Server {
            status: 400,
            message: "Bad Request".to_string(),
        };
        assert!(err.to_string().contains("400"));
    }

    #[test]
    fn display_is_single_sentence() {
        let cases: Vec<EmbyError> = vec![
            EmbyError::Unauthorized,
            EmbyError::NotFound { item: None },
            EmbyError::Transient {
                status: 500,
                retry_after: None,
            },
            EmbyError::InvalidUrl("bad".to_string()),
            EmbyError::InvalidHeader {
                name: "X".to_string(),
            },
            EmbyError::Server {
                status: 418,
                message: "teapot".to_string(),
            },
        ];
        for e in cases {
            let s = e.to_string();
            assert!(
                !s.contains('\n'),
                "variant {e:?} display contains a newline: {s:?}"
            );
            assert!(
                s.ends_with('.'),
                "variant {e:?} display does not end with '.': {s:?}"
            );
        }
    }

    #[test]
    fn decode_error_display_omits_body() {
        let source: serde_json::Error =
            serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = EmbyError::Decode {
            endpoint: "/Items".to_string(),
            source,
        };
        let s = err.to_string();
        assert!(!s.contains("not json"));
    }
}
