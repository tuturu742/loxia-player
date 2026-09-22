# Cache and offline data

`loxia-cache` owns local operational storage. It separates cache layout and eviction metadata from
user configuration and from the in-memory application state in `loxia-core`.

## Storage responsibilities

- `layout` defines cache paths and filename layout.
- `manifest` records cached media metadata.
- `lru` maintains eviction information.
- `downloads` manages durable downloads.
- `offline_index` stores data used to browse available offline content.
- `scrobble` stores playback reports that await delivery.
- `session` stores restorable session information.
- `error` defines cache-specific failures.

The player and cache worker use these services at the I/O boundary. Reducers express cache work as
effects and update connectivity and availability state from resulting events.

## Offline behaviour

Downloaded or cached data remains distinguishable from server-backed data. When connectivity is
unavailable, the UI reflects the current offline state and can use the offline index where data is
available. Deferred scrobbles remain local until a later successful delivery path processes them.

Cache failures are surfaced as application events rather than causing views to access files
directly.
