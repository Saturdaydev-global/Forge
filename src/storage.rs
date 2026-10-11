use std::{path::Path, time::Duration};

use rusqlite::{Connection, TransactionBehavior, params};

use crate::{
    Error,
    domain::{Project, ProjectName},
};

const SCHEMA_VERSION: i64 = 1;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_millis(250))?;
        let version = schema_version(&connection)?;
        validate_version(version)?;
        if version == 0 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let version = schema_version(&transaction)?;
            validate_version(version)?;
            if version == 0 {
                transaction.execute_batch(include_str!("../migrations/0001_projects.sql"))?;
                transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            transaction.commit()?;
        }
        Ok(Self { connection })
    }

    pub fn add_project(&self, name: ProjectName) -> Result<Project, Error> {
        self.connection.execute(
            "INSERT INTO projects (name) VALUES (?1)",
            params![name.as_str()],
        )?;
        Ok(Project {
            id: self.connection.last_insert_rowid(),
            name,
        })
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name FROM projects ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, name) = row?;
            Ok(Project {
                id,
                name: ProjectName::new(&name)?,
            })
        })
        .collect()
    }
}

fn schema_version(connection: &Connection) -> Result<i64, Error> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn validate_version(version: i64) -> Result<(), Error> {
    if !(0..=SCHEMA_VERSION).contains(&version) {
        return Err(Error::UnsupportedSchema {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(())
}
