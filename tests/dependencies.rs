use std::{
    path::Path,
    process::{Command, Output},
};

use forge::{Error, storage::Store};
use rusqlite::Connection;
use tempfile::TempDir;

fn cli(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forge"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn failure(output: &Output) -> String {
    assert_eq!(output.status.code(), Some(1));
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn user_version(db: &Path) -> i64 {
    Connection::open(db)
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn tables(db: &Path) -> Vec<String> {
    Connection::open(db)
        .unwrap()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|row| row.unwrap())
        .collect()
}

fn add_project(db: &Path, name: &str) -> String {
    success(&cli(db, &["project", "add", name]))
}

fn add_task(db: &Path, project: &str, title: &str) -> String {
    success(&cli(
        db,
        &[
            "task",
            "add",
            "--project",
            project,
            title,
            "--criterion",
            "Done",
        ],
    ))
}

fn dependency_count(db: &Path) -> i64 {
    Connection::open(db)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM task_dependencies", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn create_v1_database(db: &Path) {
    Connection::open(db)
        .unwrap()
        .execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY, name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200)); INSERT INTO projects (name) VALUES ('Legacy'); PRAGMA user_version = 1;",
        )
        .unwrap();
}

fn create_v2_database(db: &Path) {
    Connection::open(db)
        .unwrap()
        .execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY, name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200)); CREATE TABLE tasks (id INTEGER PRIMARY KEY, project_id INTEGER NOT NULL REFERENCES projects (id), title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200), status TEXT NOT NULL DEFAULT 'backlog' CHECK (status = 'backlog')); CREATE TABLE task_criteria (task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE, position INTEGER NOT NULL CHECK (position >= 0), body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 200), PRIMARY KEY (task_id, position)); CREATE INDEX index_tasks_project_id ON tasks (project_id); INSERT INTO projects (name) VALUES ('Legacy'); INSERT INTO tasks (project_id, title, status) VALUES (1, 'Kept', 'backlog'); INSERT INTO task_criteria (task_id, position, body) VALUES (1, 0, 'Keep me'); PRAGMA user_version = 2;",
        )
        .unwrap();
}

#[test]
fn migrates_fresh_database_to_version_3() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("fresh.sqlite");
    assert_eq!(success(&cli(&db, &["project", "list"])), "");
    assert_eq!(user_version(&db), 3);
    let names = tables(&db);
    assert!(names.contains(&"projects".to_owned()));
    assert!(names.contains(&"tasks".to_owned()));
    assert!(names.contains(&"task_criteria".to_owned()));
    assert!(names.contains(&"task_dependencies".to_owned()));
}

#[test]
fn migrates_v1_and_v2_preserving_existing_data() {
    let temp = TempDir::new().unwrap();
    let v1 = temp.path().join("v1.sqlite");
    create_v1_database(&v1);
    assert_eq!(success(&cli(&v1, &["project", "list"])), "1\tLegacy\n");
    assert_eq!(user_version(&v1), 3);
    add_task(&v1, "1", "After");
    assert_eq!(
        success(&cli(&v1, &["task", "list", "--project", "1"])),
        "1\tAfter\tbacklog\n"
    );

    let v2 = temp.path().join("v2.sqlite");
    create_v2_database(&v2);
    assert_eq!(success(&cli(&v2, &["project", "list"])), "1\tLegacy\n");
    assert_eq!(
        success(&cli(&v2, &["task", "list", "--project", "1"])),
        "1\tKept\tbacklog\n"
    );
    let show = success(&cli(&v2, &["task", "show", "1"]));
    assert!(show.contains("Title: Kept"));
    assert!(show.contains("Keep me"));
    assert_eq!(user_version(&v2), 3);
    add_task(&v2, "1", "Second");
    success(&cli(&v2, &["task", "dependency", "add", "2", "1"]));
    assert_eq!(
        success(&cli(&v2, &["task", "dependency", "list", "2"])),
        "1\n"
    );
}

#[test]
fn manages_valid_dependencies_and_persists_across_processes() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("valid.sqlite");
    add_project(&db, "Forge");
    add_task(&db, "1", "First");
    add_task(&db, "1", "Second");
    add_task(&db, "1", "Third");

    assert_eq!(
        success(&cli(&db, &["task", "dependency", "add", "3", "1"])),
        ""
    );
    assert_eq!(
        success(&cli(&db, &["task", "dependency", "add", "3", "2"])),
        ""
    );
    assert_eq!(
        success(&cli(&db, &["task", "dependency", "list", "3"])),
        "1\n2\n"
    );
    let show = success(&cli(&db, &["task", "show", "3"]));
    assert!(show.contains("Title: Third"));
    assert!(show.contains("Dependencies:"));
    let deps = show.split("Dependencies:").nth(1).unwrap();
    assert!(deps.contains('1'));
    assert!(deps.contains('2'));

    assert_eq!(
        success(&cli(&db, &["task", "dependency", "list", "3"])),
        "1\n2\n"
    );
    assert_eq!(success(&cli(&db, &["task", "dependency", "list", "1"])), "");
    let show_one = success(&cli(&db, &["task", "show", "1"]));
    assert!(show_one.contains("Dependencies:"));
    assert!(show_one.contains("(none)"));

    let store = Store::open(&db).unwrap();
    assert_eq!(store.list_dependencies(3).unwrap(), vec![1, 2]);
}

#[test]
fn rejects_cross_project_and_self_dependencies() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("isolation.sqlite");
    add_project(&db, "One");
    add_project(&db, "Two");
    add_task(&db, "1", "A");
    add_task(&db, "2", "B");

    let stderr = failure(&cli(&db, &["task", "dependency", "add", "1", "2"]));
    assert!(stderr.contains("different projects"), "{stderr}");
    let stderr = failure(&cli(&db, &["task", "dependency", "add", "2", "1"]));
    assert!(stderr.contains("different projects"), "{stderr}");
    let stderr = failure(&cli(&db, &["task", "dependency", "add", "1", "1"]));
    assert!(stderr.contains("cannot depend on itself"), "{stderr}");

    assert_eq!(success(&cli(&db, &["task", "dependency", "list", "1"])), "");
    assert_eq!(dependency_count(&db), 0);
}

#[test]
fn rejects_direct_and_indirect_cycles() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("cycles.sqlite");
    add_project(&db, "Forge");
    add_task(&db, "1", "A");
    add_task(&db, "1", "B");
    add_task(&db, "1", "C");

    success(&cli(&db, &["task", "dependency", "add", "2", "1"]));
    let stderr = failure(&cli(&db, &["task", "dependency", "add", "1", "2"]));
    assert!(stderr.contains("circular"), "{stderr}");

    success(&cli(&db, &["task", "dependency", "add", "3", "2"]));
    let stderr = failure(&cli(&db, &["task", "dependency", "add", "1", "3"]));
    assert!(stderr.contains("circular"), "{stderr}");

    assert_eq!(
        success(&cli(&db, &["task", "dependency", "list", "2"])),
        "1\n"
    );
    assert_eq!(
        success(&cli(&db, &["task", "dependency", "list", "3"])),
        "2\n"
    );
    assert_eq!(success(&cli(&db, &["task", "dependency", "list", "1"])), "");
    assert_eq!(dependency_count(&db), 2);
}

#[test]
fn rejects_duplicates_and_unknown_tasks() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("dup.sqlite");
    add_project(&db, "Forge");
    add_task(&db, "1", "A");
    add_task(&db, "1", "B");

    success(&cli(&db, &["task", "dependency", "add", "2", "1"]));
    let stderr = failure(&cli(&db, &["task", "dependency", "add", "2", "1"]));
    assert!(stderr.contains("already depends"), "{stderr}");

    for args in [
        vec!["task", "dependency", "add", "999", "1"],
        vec!["task", "dependency", "add", "1", "999"],
        vec!["task", "dependency", "add", "-1", "1"],
        vec!["task", "dependency", "list", "999"],
        vec!["task", "dependency", "remove", "999", "1"],
        vec!["task", "dependency", "remove", "1", "999"],
    ] {
        let stderr = failure(&cli(&db, &args));
        assert!(stderr.contains("does not exist"), "{stderr}");
    }
    let stderr = failure(&cli(&db, &["task", "dependency", "remove", "1", "2"]));
    assert!(stderr.contains("does not depend"), "{stderr}");
    assert_eq!(dependency_count(&db), 1);
}

#[test]
fn removes_dependencies() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("remove.sqlite");
    add_project(&db, "Forge");
    add_task(&db, "1", "A");
    add_task(&db, "1", "B");
    add_task(&db, "1", "C");

    success(&cli(&db, &["task", "dependency", "add", "3", "1"]));
    success(&cli(&db, &["task", "dependency", "add", "3", "2"]));
    assert_eq!(
        success(&cli(&db, &["task", "dependency", "remove", "3", "1"])),
        ""
    );
    assert_eq!(
        success(&cli(&db, &["task", "dependency", "list", "3"])),
        "2\n"
    );
    let show = success(&cli(&db, &["task", "show", "3"]));
    assert!(!show.split("Dependencies:").nth(1).unwrap().contains('1'));

    let stderr = failure(&cli(&db, &["task", "dependency", "remove", "3", "1"]));
    assert!(stderr.contains("does not depend"), "{stderr}");
    assert_eq!(
        success(&cli(&db, &["task", "dependency", "add", "3", "1"])),
        ""
    );
    assert_eq!(
        success(&cli(&db, &["task", "dependency", "list", "3"])),
        "1\n2\n"
    );
}

#[test]
fn enforces_atomicity_and_foreign_keys() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("atomic.sqlite");
    add_project(&db, "Forge");
    add_task(&db, "1", "A");
    add_task(&db, "1", "B");
    success(&cli(&db, &["task", "dependency", "add", "2", "1"]));

    let before = dependency_count(&db);
    for args in [
        vec!["task", "dependency", "add", "2", "1"],
        vec!["task", "dependency", "add", "1", "2"],
        vec!["task", "dependency", "add", "1", "1"],
    ] {
        assert_eq!(cli(&db, &args).status.code(), Some(1));
    }
    assert_eq!(dependency_count(&db), before);

    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    assert!(
        connection
            .execute(
                "INSERT INTO task_dependencies (task_id, prerequisite_id) VALUES (2, 999)",
                [],
            )
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO task_dependencies (task_id, prerequisite_id) VALUES (2, 2)",
                [],
            )
            .is_err()
    );
    assert_eq!(dependency_count(&db), before);
}

#[test]
fn propagates_database_errors_instead_of_masking_as_unknown() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("locked.sqlite");
    add_project(&db, "Forge");
    add_task(&db, "1", "A");

    let lock = Connection::open(&db).unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE;").unwrap();
    let output = cli(&db, &["task", "list", "--project", "1"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains("database is locked"), "{stderr}");
    assert!(!stderr.contains("does not exist"), "{stderr}");

    match Store::open(&db).and_then(|store| store.list_tasks(1)) {
        Err(Error::Database(_)) => {}
        Err(Error::UnsupportedSchema { .. }) => panic!("unexpected schema error"),
        Ok(_) => panic!("expected database error"),
        Err(other) => panic!("expected database error, got: {other}"),
    }
    lock.execute_batch("ROLLBACK;").unwrap();
    assert!(Store::open(&db).unwrap().list_tasks(1).is_ok());
}
