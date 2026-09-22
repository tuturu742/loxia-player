# Cache and offline operation

`loxia-cache` owns local data that survives a process restart. It keeps filesystem policy outside
the pure application core.

## Stored concerns

| Module | Responsibility |
| :-- | :-- |
| `layout` | Cache directory layout and safe path construction |
| `manifest` | Cached-media metadata |
| `lru` | Eviction policy |
| `downloads` | Permanent download tracking |
| `offline_index` | Browsable offline media index |
| `scrobble` | Deferred playback-report records |
| `session` | Restored application session data |
| `error` | Cache-specific errors |

Cache data is distinct from user configuration. `loxia_core::paths` selects the per-platform
locations, while `loxia-cache` defines the files and records placed beneath them.

## Offline behaviour

The runtime uses connectivity state to decide whether a request can use an available local record
or requires the Emby client. Downloaded and cached media remain available through the cache APIs;
network-only operations return through the normal effect/event path when connectivity is restored.

Cache writes, reads, eviction, session persistence, and deferred scrobbling run in workers rather
than reducers. Reducers only express the requested operation and update state from returned events.

See [`02-data-model.md`](02-data-model.md) for configuration ownership and
[`09-traceability.md`](09-traceability.md) for the modules that implement each offline concern.
