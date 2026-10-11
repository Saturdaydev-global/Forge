use std::{
    fs,
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

#[test]
fn persists_projects_across_cli_processes_and_reopens_migration() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("projects.sqlite");
    assert_eq!(success(&cli(&db, &["project", "list"])), "");
    assert_eq!(
        success(&cli(&db, &["project", "add", "  Forge  "])),
        "1\tForge\n"
    );
    assert_eq!(
        success(&cli(
            &db,
            &["project", "add", "O'Brien; DROP TABLE projects; --"]
        )),
        "2\tO'Brien; DROP TABLE projects; --\n"
    );
    assert_eq!(
        success(&cli(&db, &["project", "list"])),
        "1\tForge\n2\tO'Brien; DROP TABLE projects; --\n"
    );
    let connection = Connection::open(&db).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn invalid_names_do_not_create_a_database() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("invalid.sqlite");
    for name in ["", "  ", "a\nb", &"x".repeat(201)] {
        let output = cli(&db, &["project", "add", name]);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("project name must"));
        assert!(!db.exists());
    }
}

#[test]
fn requires_explicit_database_path() {
    let output = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["project", "list"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--db"));
}

#[test]
fn rejects_newer_schema_without_changing_database_bytes() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("future.sqlite");
    let connection = Connection::open(&db).unwrap();
    connection.execute_batch("CREATE TABLE future_data (value TEXT); INSERT INTO future_data VALUES ('keep'); PRAGMA user_version = 2;").unwrap();
    drop(connection);
    let before = fs::read(&db).unwrap();
    let output = cli(&db, &["project", "list"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("schema version 2 is unsupported"));
    assert_eq!(before, fs::read(&db).unwrap());
}

#[test]
fn failed_migration_preserves_existing_schema_and_version() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("conflict.sqlite");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE projects (original TEXT); INSERT INTO projects VALUES ('keep');",
        )
        .unwrap();
    assert!(matches!(Store::open(&db), Err(Error::Database(_))));
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT original FROM projects", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}

#[test]
fn reports_missing_parent_and_corrupt_database() {
    let temp = TempDir::new().unwrap();
    for db in [
        temp.path().join("missing/forge.sqlite"),
        temp.path().join("corrupt.sqlite"),
    ] {
        if db.file_name().unwrap() == "corrupt.sqlite" {
            fs::write(&db, b"this is not a SQLite database").unwrap();
        }
        let output = cli(&db, &["project", "list"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("database operation failed"));
    }
}

#[test]
fn reports_locked_database_without_inserting_project() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("locked.sqlite");
    drop(Store::open(&db).unwrap());
    let connection = Connection::open(&db).unwrap();
    connection.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let output = cli(&db, &["project", "add", "blocked"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("database is locked"));
    connection.execute_batch("ROLLBACK;").unwrap();
    assert!(
        Store::open(&db)
            .unwrap()
            .list_projects()
            .unwrap()
            .is_empty()
    );
}
