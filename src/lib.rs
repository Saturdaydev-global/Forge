pub mod domain;
pub mod storage;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("project name must contain 1 to 200 characters and no control characters")]
    InvalidProjectName,
    #[error("task title must contain 1 to 200 characters and no control characters")]
    InvalidTaskTitle,
    #[error("criterion must contain 1 to 200 characters and no control characters")]
    InvalidCriterion,
    #[error("task must include at least one acceptance criterion")]
    MissingCriteria,
    #[error("task status is unsupported")]
    InvalidTaskStatus,
    #[error("project {id} does not exist")]
    UnknownProject { id: i64 },
    #[error("task {id} does not exist")]
    UnknownTask { id: i64 },
    #[error(
        "database schema version {found} is unsupported; this Forge supports version {supported}"
    )]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
}
