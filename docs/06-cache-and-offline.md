# Cache and offline data

`loxia-cache` owns durable local data that is derived from server content or
needed to continue application workflows between runs.

## Modules

| Module | Responsibility |
|---|---|
| `layout` | Cache directory layout and safe path construction. |
| `downloads` | Permanent and temporary downloaded media. |
| `manifest` | Stored metadata describing cached entries. |
| `lru` | Cache eviction bookkeeping. |
| `offline_index` | Browseable offline library data. |
| `scrobble` | Playback reports retained while the server is unavailable. |
| `session` | Persisted session and history data. |
| `error` | Cache-specific errors. |

Cache workers in `loxia-player` execute filesystem work requested through
effects. `loxia-core` retains the state-machine logic for connectivity and
does not access files directly.

## Offline behaviour

The runtime distinguishes online, reconnecting, and offline conditions in its
connectivity state. Cached metadata and downloaded media remain available when
the server cannot be reached. Operations requiring a server become effects
only when connectivity permits them; buffered reports and persisted session
data are reconciled by the runtime when connectivity returns.

Cache data is disposable except where the user explicitly selects permanent
downloads or retained application state. Layout and manifest code confines
entries to the configured cache root and avoids using server-provided names as
unsafe filesystem paths.
