use forge::{
    Error,
    domain::{Criterion, ProjectName, TaskStatus, TaskTitle},
    execution::{Outcome, WorkerGuard, run_mock},
    storage::Store,
};
use rusqlite::Connection;
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn seed(db: &Path) -> Store {
    let mut store = Store::open(db).unwrap();
    store
        .add_project(ProjectName::new("Test").unwrap())
        .unwrap();
    for title in ["First", "Second"] {
        store
            .add_task(
                1,
                TaskTitle::new(title).unwrap(),
                vec![Criterion::new("Reviewed").unwrap()],
            )
            .unwrap();
    }
    store
}

#[test]
fn schedules_only_ready_tasks_with_completed_dependencies() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("forge.sqlite");
    let mut store = seed(&db);
    store.add_dependency(2, 1).unwrap();
    store.change_status(1, TaskStatus::Ready).unwrap();
    store.change_status(2, TaskStatus::Ready).unwrap();
    assert_eq!(store.ready_tasks(1).unwrap(), vec![1]);
    assert!(matches!(
        run_mock(&db, 2, false, 0),
        Err(Error::IncompleteDependencies(2))
    ));
    let id = run_mock(&db, 1, false, 0).unwrap();
    assert_eq!(store.get_task(1).unwrap().status, TaskStatus::Review);
    assert!(store.ready_tasks(1).unwrap().is_empty());
    assert!(matches!(
        store.add_dependency(1, 2),
        Err(Error::FrozenDependencies(1))
    ));
    store.change_status(1, TaskStatus::Completed).unwrap();
    assert_eq!(store.ready_tasks(1).unwrap(), vec![2]);
    assert!(store.change_status(1, TaskStatus::Running).is_err());
    assert_eq!(
        store
            .events(id)
            .unwrap()
            .iter()
            .map(|e| e.0.as_str())
            .collect::<Vec<_>>(),
        vec!["started", "succeeded"]
    );
    assert!(matches!(
        store.finish_run(id, Outcome::Failed),
        Err(Error::InactiveRun(_))
    ));
}

#[test]
fn cancellation_is_durable_and_overrides_success() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("forge.sqlite");
    let mut store = seed(&db);
    store.change_status(1, TaskStatus::Ready).unwrap();
    let guard = WorkerGuard::acquire(&db).unwrap();
    let id = store.begin_run(1, "mock", "simulated", &guard).unwrap();
    assert!(matches!(WorkerGuard::acquire(&db), Err(Error::WorkerBusy)));
    let mut other = Store::open(&db).unwrap();
    other.request_cancel(id).unwrap();
    store.finish_run(id, Outcome::Succeeded).unwrap();
    assert_eq!(other.get_task(1).unwrap().status, TaskStatus::Cancelled);
    assert_eq!(other.runs(1).unwrap()[0].status, "cancelled");
    assert!(other.request_cancel(id).is_err());
}

#[test]
fn crashed_worker_releases_lock_and_recovery_never_replays() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("forge.sqlite");
    let mut store = seed(&db);
    store.change_status(1, TaskStatus::Ready).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("--db")
        .arg(&db)
        .args(["run", "mock", "1", "--delay-ms", "10000"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while store.runs(1).unwrap().is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let active = !store.runs(1).unwrap().is_empty();
    let refused = matches!(WorkerGuard::acquire(&db), Err(Error::WorkerBusy));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(active);
    assert!(refused);
    let guard = WorkerGuard::acquire(&db).unwrap();
    assert_eq!(store.recover(&guard).unwrap(), 1);
    assert_eq!(store.recover(&guard).unwrap(), 0);
    assert_eq!(store.runs(1).unwrap()[0].status, "interrupted");
    assert_eq!(store.get_task(1).unwrap().status, TaskStatus::Failed);
    assert!(store.ready_tasks(1).unwrap().is_empty());
}

#[test]
fn v3_migration_preserves_tasks_criteria_dependencies_and_foreign_keys() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("forge.sqlite");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(include_str!("../migrations/0001_projects.sql"))
        .unwrap();
    c.execute_batch(include_str!("../migrations/0002_tasks.sql"))
        .unwrap();
    c.execute_batch(include_str!("../migrations/0003_task_dependencies.sql"))
        .unwrap();
    c.execute_batch("INSERT INTO projects VALUES(1,'Old'); INSERT INTO tasks VALUES(1,1,'A','backlog'),(2,1,'B','backlog'); INSERT INTO task_criteria VALUES(1,0,'Keep'),(2,0,'Also keep'); INSERT INTO task_dependencies VALUES(2,1); PRAGMA user_version=3;").unwrap();
    drop(c);
    let store = Store::open(&db).unwrap();
    assert_eq!(store.list_dependencies(2).unwrap(), vec![1]);
    assert_eq!(store.get_task(1).unwrap().criteria[0].as_str(), "Keep");
    let c = Connection::open(&db).unwrap();
    assert!(
        !c.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
}

#[test]
fn late_migration_failure_rolls_back_table_rebuild() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("forge.sqlite");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(include_str!("../migrations/0001_projects.sql"))
        .unwrap();
    c.execute_batch(include_str!("../migrations/0002_tasks.sql"))
        .unwrap();
    c.execute_batch(include_str!("../migrations/0003_task_dependencies.sql"))
        .unwrap();
    c.execute_batch("INSERT INTO projects VALUES(1,'Old'); INSERT INTO tasks VALUES(1,1,'A','backlog'); CREATE TABLE runs(keep TEXT); PRAGMA user_version=3;").unwrap();
    assert!(Store::open(&db).is_err());
    assert_eq!(
        c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert!(c.execute("UPDATE tasks SET status='ready'", []).is_err());
    assert_eq!(
        c.query_row("SELECT title FROM tasks", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "A"
    );
}
