pub mod domain;
pub mod storage;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("project name must contain 1 to 200 characters and no control characters")]
    InvalidProjectName,
    #[error(
        "database schema version {found} is unsupported; this Forge supports version {supported}"
    )]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
}
