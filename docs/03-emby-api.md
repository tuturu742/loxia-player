# Emby API adapter

`loxia-emby` is the asynchronous adapter for Emby servers. It keeps HTTP,
WebSocket, authentication, DTOs, retrying, endpoint details, and stream URL
construction outside `loxia-core`.

## Client structure

- `auth` handles authentication material.
- `client` owns client setup and request execution.
- `dto` contains server-response representations and conversions.
- `endpoints` groups API operations by resource.
- `query` constructs library and search queries.
- `retry` applies retry policy.
- `stream` creates direct-play and transcode stream requests.
- `ws` handles Emby WebSocket communication.

Endpoint modules cover items, search, favourites, playlists, playback,
discography, images, lyrics, downloads, instant mixes, and probing.

## Boundary rules

The adapter converts server DTOs to `loxia-core` model types before data reaches
the reducer. Tokens and stream URLs are sensitive: logging and debug output use
redacted representations. Network failures become adapter errors and runtime
events rather than panics.

Playback reporting uses the playback endpoint for start, progress, played, and
stopped notifications. Stream construction selects direct or transcoded URLs
from the server playback information and the configured transcode settings.

Fixtures under `crates/loxia-emby/tests/fixtures` represent scrubbed server
responses. They contain neither credentials nor private-network addresses.
