use std::{
    path::Path,
    process::{Command, Output},
};

use forge::{
    domain::{Criterion, TaskTitle},
    storage::Store,
};
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

fn create_v1_database(db: &Path) {
    let connection = Connection::open(db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY, name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200)); INSERT INTO projects (name) VALUES ('Legacy'); PRAGMA user_version = 1;",
        )
        .unwrap();
}

#[test]
fn migrates_v1_to_v2_without_losing_projects() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("migrate.sqlite");
    create_v1_database(&db);

    let output = cli(&db, &["project", "list"]);
    assert_eq!(success(&output), "1\tLegacy\n");
    assert_eq!(user_version(&db), 2);

    let store = Store::open(&db).unwrap();
    let projects = store.list_projects().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name.as_str(), "Legacy");

    let connection = Connection::open(&db).unwrap();
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert!(tables.contains(&"tasks".to_owned()));
    assert!(tables.contains(&"task_criteria".to_owned()));
}

#[test]
fn persists_tasks_across_cli_processes_and_preserves_order() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("tasks.sqlite");
    assert_eq!(
        success(&cli(&db, &["project", "add", "Forge"])),
        "1\tForge\n"
    );
    assert_eq!(
        success(&cli(
            &db,
            &[
                "task",
                "add",
                "--project",
                "1",
                "  Implement routing  ",
                "--criterion",
                "Routes tasks",
                "--criterion",
                "Rejects paid fallback"
            ]
        )),
        "1\tImplement routing\n"
    );
    assert_eq!(
        success(&cli(&db, &["task", "list", "--project", "1"])),
        "1\tImplement routing\tbacklog\n"
    );
    let show = success(&cli(&db, &["task", "show", "1"]));
    assert!(show.contains("Task 1"));
    assert!(show.contains("Project: 1"));
    assert!(show.contains("Title: Implement routing"));
    assert!(show.contains("Status: backlog"));
    assert!(show.find("1. Routes tasks").unwrap() < show.find("2. Rejects paid fallback").unwrap());

    // Reopen in a separate process and confirm durability.
    assert_eq!(
        success(&cli(&db, &["task", "list", "--project", "1"])),
        "1\tImplement routing\tbacklog\n"
    );
    let store = Store::open(&db).unwrap();
    let tasks = store.list_tasks(1).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title.as_str(), "Implement routing");
    assert_eq!(
        tasks[0]
            .criteria
            .iter()
            .map(|criterion| criterion.as_str())
            .collect::<Vec<_>>(),
        vec!["Routes tasks", "Rejects paid fallback"]
    );
    assert_eq!(user_version(&db), 2);
}

#[test]
fn isolates_tasks_between_projects() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("isolation.sqlite");
    success(&cli(&db, &["project", "add", "One"]));
    success(&cli(&db, &["project", "add", "Two"]));
    success(&cli(
        &db,
        &[
            "task",
            "add",
            "--project",
            "1",
            "First",
            "--criterion",
            "Done",
        ],
    ));
    success(&cli(
        &db,
        &[
            "task",
            "add",
            "--project",
            "2",
            "Second",
            "--criterion",
            "Checked",
        ],
    ));

    assert_eq!(
        success(&cli(&db, &["task", "list", "--project", "1"])),
        "1\tFirst\tbacklog\n"
    );
    assert_eq!(
        success(&cli(&db, &["task", "list", "--project", "2"])),
        "2\tSecond\tbacklog\n"
    );
    let show = success(&cli(&db, &["task", "show", "2"]));
    assert!(show.contains("Project: 2"));
    assert!(show.contains("Title: Second"));
}

#[test]
fn rejects_unknown_projects_and_tasks() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("unknown.sqlite");
    success(&cli(&db, &["project", "add", "Forge"]));

    for id in ["999", "0", "-1"] {
        let stderr = failure(&cli(
            &db,
            &[
                "task",
                "add",
                "--project",
                id,
                "Title",
                "--criterion",
                "Done",
            ],
        ));
        assert!(stderr.contains("does not exist"), "{stderr}");
        let stderr = failure(&cli(&db, &["task", "list", "--project", id]));
        assert!(stderr.contains("does not exist"), "{stderr}");
    }
    let stderr = failure(&cli(&db, &["task", "show", "999"]));
    assert!(stderr.contains("task 999 does not exist"), "{stderr}");

    // Failed inserts must not create tasks.
    assert_eq!(success(&cli(&db, &["task", "list", "--project", "1"])), "");
    let connection = Connection::open(&db).unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn rejects_invalid_titles_and_criteria_without_inserting() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("invalid.sqlite");
    success(&cli(&db, &["project", "add", "Forge"]));

    for title in ["", "  ", "a\nb", &"x".repeat(201)] {
        let stderr = failure(&cli(
            &db,
            &[
                "task",
                "add",
                "--project",
                "1",
                title,
                "--criterion",
                "Done",
            ],
        ));
        assert!(stderr.contains("task title must"), "{stderr}");
    }
    let stderr = failure(&cli(&db, &["task", "add", "--project", "1", "Title"]));
    assert!(stderr.contains("at least one"), "{stderr}");

    for criterion in ["", "  ", "a\nb", &"x".repeat(201)] {
        let stderr = failure(&cli(
            &db,
            &[
                "task",
                "add",
                "--project",
                "1",
                "Title",
                "--criterion",
                criterion,
            ],
        ));
        assert!(stderr.contains("criterion must"), "{stderr}");
    }

    assert_eq!(success(&cli(&db, &["task", "list", "--project", "1"])), "");
    assert!(Store::open(&db).unwrap().list_tasks(1).unwrap().is_empty());
}

#[test]
fn enforces_foreign_keys_and_atomic_inserts() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("atomic.sqlite");
    success(&cli(&db, &["project", "add", "Forge"]));

    let mut store = Store::open(&db).unwrap();
    let task = store
        .add_task(
            1,
            TaskTitle::new("Atomic").unwrap(),
            vec![Criterion::new("First").unwrap()],
        )
        .unwrap();
    assert_eq!(task.id, 1);

    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    let direct = connection.execute(
        "INSERT INTO tasks (project_id, title, status) VALUES (999, 'Bad', 'backlog')",
        [],
    );
    assert!(direct.is_err());

    let direct = connection.execute(
        "INSERT INTO task_criteria (task_id, position, body) VALUES (999, 0, 'Bad')",
        [],
    );
    assert!(direct.is_err());

    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
