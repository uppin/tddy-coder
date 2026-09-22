//! Errors surfaced by the session catalog.

/// Failure modes of the session catalog store and read path.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// Underlying SQLite / sqlx failure.
    #[error("catalog sqlite error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// A cross-process reader waited for the populate marker but it never appeared in time.
    #[error("timed out waiting for catalog to be populated")]
    PopulateTimeout,

    /// Serialization of a catalog entry to/from JSON failed.
    #[error("catalog json error: {0}")]
    Json(#[from] serde_json::Error),
}
