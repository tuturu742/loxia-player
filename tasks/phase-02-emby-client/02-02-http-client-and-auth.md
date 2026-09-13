# 02-02 · HTTP client and auth

**Phase:** 02 — Emby client · **Agent:** B · **Size:** M
**Prerequisites:** `02-03`
**Reference:** `docs/03-emby-api.md` §§1–2

**Note:** this task's real prerequisite is `02-03` (not `02-01` as an earlier draft had it) — every
error path here (`InvalidUrl`, `InvalidHeader`, `Unauthorized`) returns `EmbyError`, which `02-03`
defines. `02-03` has no real dependency on this task, so implement it first. See
`docs/12-decisions.md`.

## Goal
Build `EmbyClient` — base URL handling, the Emby authorization header, custom-header injection, and
login. Every later endpoint task builds on it.

## Files
- `crates/loxia-emby/src/client.rs`
- `crates/loxia-emby/src/auth.rs`

## Specification

```
pub struct EmbyClient { /* reqwest::Client, base: Url, auth: AuthState, headers: HeaderMap */ }

impl EmbyClient {
    pub fn new(cfg: &ServerConfig) -> Result<Self, EmbyError>;
    pub fn url(&self, path: &str) -> Url;            // {base}/emby/{path}
    pub fn get(&self, path: &str) -> RequestBuilder; // headers pre-applied
    pub fn post(&self, path: &str) -> RequestBuilder;
    pub fn delete(&self, path: &str) -> RequestBuilder;
    pub fn user_id(&self) -> &UserId;
}
```

**Base URL normalisation** in `new`: trim whitespace, strip trailing `/`, reject a scheme other than
`http`/`https` with `EmbyError::InvalidUrl`. Append `/emby` in `url()`, not in the stored base — the
stored base is what gets shown to the user in Settings.

**Auth header**, built once and stored in the `HeaderMap`:
```
X-Emby-Authorization: MediaBrowser Client="loxia", Device="<hostname>",
                      DeviceId="<cfg.device_id>", Version="<CARGO_PKG_VERSION>", Token="<token>"
X-Emby-Token: <token>
```
Values are quoted and any `"` in the hostname is escaped. `device_id` comes from config and is
already guaranteed non-empty by `01-02`.

**Custom headers:** merge `cfg.custom_headers` into the same `HeaderMap` at construction. Reserved
names (`authorization`, `x-emby-authorization`, `host`, `content-length`) are skipped — `01-02`
already strips them, so reaching one here is a bug worth a `warn!`. A header name or value that is
not valid HTTP yields `EmbyError::InvalidHeader { name }`.

**Client settings:** `connect_timeout` 10 s, `timeout` 30 s, `pool_idle_timeout` 90 s,
`user_agent("loxia/<version>")`. `rustls` only.

**`auth.rs`:**
```
pub async fn authenticate(base: &Url, headers: &HeaderMap, user: &str, pw: &str)
    -> Result<AuthResult, EmbyError>;          // AuthResult { user_id, access_token, server_id }
pub async fn validate_token(client: &EmbyClient) -> Result<UserInfo, EmbyError>;
```
`authenticate` posts to `/Users/AuthenticateByName` with `{ "Username", "Pw" }`. `validate_token`
gets `/Users/{user_id}`; a 401 maps to `EmbyError::Unauthorized`, which the UI turns into the
re-login flow rather than a crash.

**Redaction:** `EmbyClient`'s `Debug` prints the base URL and header **names** only. A test asserts
the token never appears.

## Acceptance
Tests using `wiremock`:
- `sends_expected_auth_headers` — asserts the exact `X-Emby-Authorization` format and `X-Emby-Token`.
- `merges_two_custom_headers`
- `skips_reserved_custom_header`
- `invalid_header_value_is_an_error`
- `base_url_normalisation` (table test): trailing slash, `HTTP://`, `ftp://` rejected, whitespace.
- `url_appends_emby_prefix`
- `authenticate_returns_token`
- `validate_token_401_maps_to_unauthorized`
- `client_debug_redacts_token`

## Done when
The global DoD in `tasks/README.md` is satisfied.
