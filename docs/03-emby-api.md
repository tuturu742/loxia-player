# Emby API integration

`loxia-emby` contains the Emby integration boundary. It owns authentication, request construction,
retry handling, DTO conversion, endpoint modules, stream selection, and WebSocket support.

## Client structure

`client`, `auth`, `query`, `retry`, `stream`, and `ws` provide shared client services. The `dto`
module represents server responses, while `endpoints` groups endpoint-specific operations for
discography, downloads, favourites, images, instant mixes, items, lyrics, playback, playlists,
probe, and search.

The endpoint layer returns workspace domain values rather than exposing DTOs to rendering and state
code. This keeps Emby's wire format at the integration boundary.

## Authentication and privacy

Authentication data belongs to configured server profiles. Access tokens and stream URLs are
treated as sensitive values: diagnostic formatting and user-visible errors do not disclose them.
Fixture data contains no real tokens or private network addresses; CI checks this invariant.

## Streaming and playback reporting

The client obtains playback information and stream URLs, then the player coordinates audio loading
and playback reporting. Playback start, progress, played, and stopped reports use the playback
endpoint. Stream choices account for direct playback and configured transcoding quality.

## Error handling

`EmbyError` describes failures without exposing credentials. Retry behaviour is centralised so
endpoint code follows one policy. Network state is represented in core state and rendered by the
UI instead of being inferred independently by individual views.
