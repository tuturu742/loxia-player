# 02-03 · Errors and retry

**Phase:** 02 — Emby client · **Agent:** B · **Size:** S
**Prerequisites:** `02-01`
**Reference:** `docs/03-emby-api.md` §11

**Note:** this task's real prerequisite is `02-01` only, not `02-02` as an earlier draft had it.
Neither `EmbyError` nor `with_retry` reference `EmbyClient` — both are fully generic (`with_retry`
is `async fn with_retry<F, Fut, T>(op: F)`, and classification works on a bare `reqwest::Error`).
`02-02` is the one with the real dependency: it needs `EmbyError` to exist for its own return
types, so implement this task **first**, ahead of `02-02`, despite the filename order. See
`docs/12-decisions.md`.

## Goal
Define `EmbyError` and the retry policy so every later endpoint gets consistent classification and
backoff without repeating the logic.

## Files
- `crates/loxia-emby/src/error.rs`
- `crates/loxia-emby/src/retry.rs`

## Specification

```
#[derive(Debug, thiserror::Error)]
pub enum EmbyError {
    Offline { source: reqwest::Error },
    Unauthorized,
    NotFound { item: Option<ItemId> },
    Transient { status: u16, retry_after: Option<Duration> },
    Decode { endpoint: String, source: serde_json::Error },
    InvalidUrl(String),
    InvalidHeader { name: String },
    Server { status: u16, message: String },
}
```

Classification from a `reqwest` result:

| Condition | Variant |
| :-- | :-- |
| `err.is_connect()`, `is_timeout()`, or a DNS failure | `Offline` |
| 401, 403 | `Unauthorized` |
| 404 | `NotFound` |
| 429, 500–599 | `Transient` (read `Retry-After` when present) |
| other 4xx | `Server` |
| body parse failure | `Decode` |

```
pub async fn with_retry<F, Fut, T>(op: F) -> Result<T, EmbyError>
where F: Fn(u32) -> Fut, Fut: Future<Output = Result<T, EmbyError>>;
```
- **Idempotent GETs only.** Mutating calls must not use this — a retried playlist add would
  duplicate entries. Enforce it by taking the helper only in the GET paths and documenting the rule
  on the function.
- Up to 3 attempts. Backoff `250 ms · 2^n` with ±25 % jitter, capped at 5 s. Honour `Retry-After`
  when it is larger.
- Retry only `Transient`. `Offline` returns immediately — the connectivity state machine
  (`08-06`) owns reconnection; retrying here would just stall the UI.
- Log each retry at `debug` with the attempt number and endpoint.

`Display` for every variant is a **single user-facing sentence** with no jargon, suitable for a
toast. `Decode` must not include the body — log that at `debug` instead.

## Acceptance
- `classify_table` — one assertion per row of the table above.
- `retries_transient_three_times_then_gives_up`
- `does_not_retry_offline`
- `does_not_retry_unauthorized`
- `honours_retry_after_header`
- `backoff_is_jittered_and_capped` — 100 samples, all within `[0.75·d, 1.25·d]` and ≤ 5 s.
- `display_is_single_sentence` — every variant's `Display` contains no newline and ends with `.`.
- `decode_error_display_omits_body`

## Done when
The global DoD in `tasks/README.md` is satisfied.
