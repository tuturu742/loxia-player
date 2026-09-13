//! CacheError.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("io error: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// `assert_within`'s own guard — the one thing standing between a hostile server-supplied
    /// filename and deleting or overwriting something outside the cache/download root.
    #[error("refused: {target} escapes the cache root {root}.")]
    PathEscapesRoot { root: PathBuf, target: PathBuf },
    /// `sanitize_component`'s collision handling gave up after ` (2)` through ` (99)` all
    /// belonged to a different item — a pathological case (100 distinct items sanitizing to the
    /// exact same name), not one to loop over forever.
    #[error("too many filename collisions for {0}.")]
    TooManyCollisions(PathBuf),
}
