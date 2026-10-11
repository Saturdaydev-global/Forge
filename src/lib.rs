pub mod domain;
pub mod execution;
pub mod storage;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("task transition from {from} to {to} is not allowed")]
    InvalidTransition { from: String, to: String },
    #[error("task {0} has incomplete prerequisites")]
    IncompleteDependencies(i64),
    #[error("dependencies cannot change while task {0} is running, in review or completed")]
    FrozenDependencies(i64),
    #[error("run {0} does not exist or is no longer running")]
    InactiveRun(i64),
    #[error("another worker holds the database execution lock")]
    WorkerBusy,
    #[error("filesystem or process operation failed: {0}")]
    Io(#[from] std::io::Error),
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
    #[error("task {task} cannot depend on itself")]
    SelfDependency { task: i64 },
    #[error("tasks {task} and {prerequisite} belong to different projects")]
    CrossProjectDependency { task: i64, prerequisite: i64 },
    #[error("circular task dependency detected")]
    CircularDependency,
    #[error("task {task} already depends on {prerequisite}")]
    DuplicateDependency { task: i64, prerequisite: i64 },
    #[error("task {task} does not depend on {prerequisite}")]
    UnknownDependency { task: i64, prerequisite: i64 },
    #[error(
        "database schema version {found} is unsupported; this Forge supports version {supported}"
    )]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
}
