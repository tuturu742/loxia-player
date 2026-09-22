# Emby integration

`loxia-emby` is the workspace's asynchronous Emby client. It owns authentication, HTTP client
construction, retries, endpoint requests, DTOs, query construction, stream URLs, and WebSocket
support.

## Client layers

- `auth` handles Emby authentication data.
- `client` owns configured HTTP access.
- `dto` represents Emby response envelopes, items, media, and ticks.
- `endpoints` groups requests for discography, downloads, favourites, images, instant mixes,
  items, lyrics, playback, playlists, probing, and search.
- `query` builds library and search queries.
- `stream` builds direct-play and transcode stream URLs.
- `retry` applies request retry policy.
- `ws` handles Emby WebSocket communication.

DTO conversion produces `loxia-core` model types so the rest of the application does not depend on
Emby response shapes.

## Authentication and diagnostics

Tokens are request credentials, not log data. Client errors and debug representations avoid
exposing tokens and stream URLs. Fixtures contain no non-empty access-token values or private
network addresses.

## Playback integration

The client obtains playback information and stream URLs, reports playback progress and completion,
and exposes media metadata needed by the audio and UI layers. `loxia-player` workers turn these
operations into runtime events; neither reducers nor TUI views call endpoint functions directly.

See [`05-audio-engine.md`](05-audio-engine.md) for the playback boundary and
[`10-testing-and-ci.md`](10-testing-and-ci.md) for fixture checks.
