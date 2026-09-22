# Emby API integration

`loxia-emby` is the workspace's Emby transport crate. It translates HTTP and WebSocket traffic into
the domain values consumed by the runtime.

## Client structure

`client` and `auth` establish authenticated requests. `retry` applies the client retry policy, and
`error` exposes transport failures without placing credentials or stream URLs in display output.
`dto` contains response representations and conversion support.

The `endpoints` module groups requests by server capability:

- `items`, `search`, `discography`, `favorites`, `instant_mix`, and `playlists` browse media;
- `playback` reports playback lifecycle and progress;
- `lyrics` and `images` retrieve supplementary media data;
- `download` obtains download data;
- `probe` verifies server connectivity.

`query` constructs item-query parameters. `stream` constructs direct and transcoded stream requests.
`ws` provides WebSocket support for server-originated changes.

## Privacy and diagnostics

Authentication values and stream URLs are treated as secrets. Diagnostic formatting redacts them,
and fixtures contain no usable credentials or private network addresses. The CI fixture scan
enforces this rule.

## API verification

Endpoint and query tests use captured JSON fixtures in
`crates/loxia-emby/tests/fixtures`. Snapshot tests cover request and query construction where a
stable textual representation is useful. The `probe` example is a small client probe for manual
server investigation.

See [`10-testing-and-ci.md`](10-testing-and-ci.md) for the test commands that run these checks.
