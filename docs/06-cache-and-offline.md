# Cache and offline data

`loxia-cache` owns on-disk data that supports cached playback, downloads, offline browsing,
scrobble buffering, and session persistence.

## Modules

| Module | Responsibility |
|---|---|
| `layout` | Resolves cache files and directories. |
| `manifest` | Records cached and downloaded media metadata. |
| `lru` | Selects evictable cache entries. |
| `downloads` | Manages durable downloads. |
| `offline_index` | Stores browsable offline library data. |
| `scrobble` | Buffers playback reports for later submission. |
| `session` | Persists restorable application session data. |
| `error` | Defines cache errors. |

## Ownership and behaviour

The cache worker, owned by `loxia-player`, performs filesystem operations requested by core
effects. Cache state remains separate from configuration and from the Emby client's HTTP layer.

Offline data represents locally available information and media. Connectivity reducers select
online or offline behaviour; the UI renders that state without inspecting cache files directly.
Queued scrobbles and session data survive temporary network unavailability and are handled by
their respective workers when connectivity permits.

See [`02-data-model.md`](02-data-model.md) for configuration and state ownership and
[`04-state-and-input.md`](04-state-and-input.md) for worker effects.
