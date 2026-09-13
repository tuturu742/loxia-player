# 10-12 · WebSocket remote control

**Phase:** 10 — Polish · **Agent:** B · **Size:** M
**Prerequisites:** `02-02`, `08-06`
**Reference:** `docs/03-emby-api.md` §10

## Goal
Connect to Emby's WebSocket so other clients can control loxia and so library changes invalidate
stale views.

## Files
- `crates/loxia-emby/src/ws.rs`
- `crates/loxia-player/src/workers/network.rs` (extend)

## Specification

Connect to `wss://{host}/embywebsocket?api_key={token}&deviceId={device_id}` with the custom headers
applied to the handshake — a reverse proxy will reject the upgrade without them.

**Inbound message handling:**
| `MessageType` | Result |
| :-- | :-- |
| `Playstate` (`PlayPause`, `NextTrack`, `PreviousTrack`, `Stop`, `Seek`) | the matching `Action::Player` |
| `GeneralCommand` (`SetVolume`, `Mute`, `Unmute`) | the matching `Action::Player` |
| `GeneralCommand` (`DisplayMessage`) | `Action::System(Toast)` |
| `LibraryChanged` | mark affected columns `LoadState::Idle` |
| `UserDataChanged` | update favourite and played flags in place |
| `ForceKeepAlive` | adjust the keepalive interval |

**`LibraryChanged` must not trigger a refetch storm.** Mark columns `Idle` and let them reload when
next viewed. A library scan finishing on a large server emits this repeatedly, and refetching
everything each time would hammer the server at the worst moment.

**Keepalive** every 30 s. **Reconnect** with jittered exponential backoff, 1 s → 30 s, resetting on
a successful connection. Stop reconnecting entirely while `connectivity == Offline`; the
connectivity probe (task `08-06`) owns that decision and two independent reconnect loops would
fight.

**The WebSocket is strictly optional.** Any failure — handshake rejection, TLS error, a proxy that
does not support upgrades — is logged at `warn` **once** and never blocks browsing or playback. Not
every deployment allows WebSockets, and those users must still get a fully working player.

`ui.enable_websocket = false` skips it entirely.

**Message parsing is defensive:** an unknown `MessageType` is ignored at `debug`, and a malformed
payload does not drop the connection.

## Acceptance
- `handshake_includes_custom_headers`
- `playstate_message_mapping` — table test over all five.
- `general_command_mapping`
- `display_message_becomes_toast`
- `library_changed_marks_idle_not_refetch` — assert zero network effects are emitted.
- `user_data_changed_updates_in_place`
- `keepalive_sent_every_30s`
- `reconnect_backoff_is_jittered_and_capped`
- `no_reconnect_while_offline`
- `handshake_failure_is_non_fatal_and_warns_once`
- `unknown_message_type_is_ignored`
- `malformed_payload_does_not_drop_connection`
- `disabled_by_config_skips_connection`

## Done when
The global DoD in `tasks/README.md` is satisfied.
