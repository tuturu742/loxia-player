# Emby API integration

`loxia-emby` is the workspace boundary for communication with Emby servers.

## Client responsibilities

The crate provides authentication, an HTTP client, retry handling, query construction, REST endpoint
modules, stream URL construction, and WebSocket support. Endpoint modules cover items, search,
playlists, favourites, images, lyrics, playback reporting, discography, instant mixes, downloads,
and server probing.

DTO modules isolate Emby response shapes from the domain types used elsewhere in the workspace.
Conversions produce `loxia-core` models instead of exposing transport payloads to reducers or views.

## Authentication and safety

Tokens remain inside the client and authenticated request construction. Logging and diagnostic
representations redact tokens and stream URLs. Fixtures contain representative responses without
credentials or private network addresses; CI scans the fixture directory for token-shaped values.

## Playback integration

The client builds stream and transcode URLs, while `loxia-audio` loads the resulting redacted URLs
into libmpv. Playback-start, progress, and stop reporting use the playback endpoint module.

Network results enter the runtime as events. The reducer and worker boundary is documented in
[`04-state-and-input.md`](04-state-and-input.md).
