# Cache and offline data

`loxia-cache` owns local storage formats and cache maintenance.

## Stored data

The crate contains modules for cache layout, manifests, LRU eviction, downloads, offline indexes,
scrobble buffering, and session persistence. It keeps filesystem concerns outside `loxia-core`.

Downloads and cached media use the layout and manifest modules to locate content and account for
storage. The LRU module selects evictable cached content. Permanent downloads remain distinct from
evictable cache entries.

## Offline operation

The offline index stores browseable metadata for locally available content. Connectivity state in
`loxia-core` determines whether reducers request remote work or use local data. Pending scrobbles
persist until the network worker can submit them.

Session persistence records restorable application state through the cache boundary. Reducers
continue to operate on domain state and effects rather than directly opening files.

## Ownership

`loxia-player` cache and download workers execute storage effects. `loxia-tui` renders storage and
connectivity state but does not manipulate storage directly.
