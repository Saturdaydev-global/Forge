use std::{path::Path, time::Duration};

use rusqlite::{Connection, TransactionBehavior, params};

use crate::{
    Error,
    domain::{Criterion, Project, ProjectName, Task, TaskStatus, TaskTitle},
};

const SCHEMA_VERSION: i64 = 3;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_millis(250))?;
        connection.pragma_update(None, "foreign_keys", 1)?;
        let version = schema_version(&connection)?;
        validate_version(version)?;
        if version < SCHEMA_VERSION {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.pragma_update(None, "foreign_keys", 1)?;
            let version = schema_version(&transaction)?;
            validate_version(version)?;
            if version == 0 {
                transaction.execute_batch(include_str!("../migrations/0001_projects.sql"))?;
            }
            if version <= 1 {
                transaction.execute_batch(include_str!("../migrations/0002_tasks.sql"))?;
            }
            if version <= 2 {
                transaction
                    .execute_batch(include_str!("../migrations/0003_task_dependencies.sql"))?;
            }
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
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

    pub fn add_task(
        &mut self,
        project_id: i64,
        title: TaskTitle,
        criteria: Vec<Criterion>,
    ) -> Result<Task, Error> {
        if criteria.is_empty() {
            return Err(Error::MissingCriteria);
        }
        if !self.project_exists(project_id)? {
            return Err(Error::UnknownProject { id: project_id });
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO tasks (project_id, title, status) VALUES (?1, ?2, 'backlog')",
            params![project_id, title.as_str()],
        )?;
        let id = transaction.last_insert_rowid();
        for (position, criterion) in criteria.iter().enumerate() {
            transaction.execute(
                "INSERT INTO task_criteria (task_id, position, body) VALUES (?1, ?2, ?3)",
                params![id, position as i64, criterion.as_str()],
            )?;
        }
        transaction.commit()?;
        Ok(Task {
            id,
            project_id,
            title,
            status: TaskStatus::Backlog,
            criteria,
        })
    }

    pub fn list_tasks(&self, project_id: i64) -> Result<Vec<Task>, Error> {
        if !self.project_exists(project_id)? {
            return Err(Error::UnknownProject { id: project_id });
        }
        let mut statement = self
            .connection
            .prepare("SELECT id, title, status FROM tasks WHERE project_id = ?1 ORDER BY id")?;
        let rows = statement.query_map(params![project_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut tasks = Vec::new();
        for row in rows {
            let (id, title, status) = row?;
            tasks.push(Task {
                id,
                project_id,
                title: TaskTitle::new(&title).map_err(|_| Error::InvalidTaskTitle)?,
                status: status.parse()?,
                criteria: self.load_criteria(id)?,
            });
        }
        Ok(tasks)
    }

    pub fn get_task(&self, id: i64) -> Result<Task, Error> {
        let (project_id, title, status): (i64, String, String) = self
            .connection
            .query_row(
                "SELECT project_id, title, status FROM tasks WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Error::UnknownTask { id },
                other => Error::Database(other),
            })?;
        Ok(Task {
            id,
            project_id,
            title: TaskTitle::new(&title).map_err(|_| Error::InvalidTaskTitle)?,
            status: status.parse()?,
            criteria: self.load_criteria(id)?,
        })
    }

    pub fn add_dependency(&mut self, task_id: i64, prerequisite_id: i64) -> Result<(), Error> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task_project = task_project_id(&transaction, task_id)?;
        let prerequisite_project = task_project_id(&transaction, prerequisite_id)?;
        if task_id == prerequisite_id {
            return Err(Error::SelfDependency { task: task_id });
        }
        if task_project != prerequisite_project {
            return Err(Error::CrossProjectDependency {
                task: task_id,
                prerequisite: prerequisite_id,
            });
        }
        if dependency_exists(&transaction, task_id, prerequisite_id)? {
            return Err(Error::DuplicateDependency {
                task: task_id,
                prerequisite: prerequisite_id,
            });
        }
        if reaches(&transaction, prerequisite_id, task_id)? {
            return Err(Error::CircularDependency);
        }
        transaction.execute(
            "INSERT INTO task_dependencies (task_id, prerequisite_id) VALUES (?1, ?2)",
            params![task_id, prerequisite_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_dependency(&mut self, task_id: i64, prerequisite_id: i64) -> Result<(), Error> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        task_project_id(&transaction, task_id)?;
        task_project_id(&transaction, prerequisite_id)?;
        let removed = transaction.execute(
            "DELETE FROM task_dependencies WHERE task_id = ?1 AND prerequisite_id = ?2",
            params![task_id, prerequisite_id],
        )?;
        if removed == 0 {
            return Err(Error::UnknownDependency {
                task: task_id,
                prerequisite: prerequisite_id,
            });
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_dependencies(&self, task_id: i64) -> Result<Vec<i64>, Error> {
        task_project_id(&self.connection, task_id)?;
        let mut statement = self.connection.prepare(
            "SELECT prerequisite_id FROM task_dependencies WHERE task_id = ?1 ORDER BY prerequisite_id",
        )?;
        let rows = statement.query_map(params![task_id], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Error::Database)
    }

    fn project_exists(&self, id: i64) -> Result<bool, Error> {
        match self
            .connection
            .query_row("SELECT 1 FROM projects WHERE id = ?1", params![id], |_| {
                Ok(())
            }) {
            Ok(()) => Ok(true),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(error) => Err(Error::Database(error)),
        }
    }

    fn load_criteria(&self, task_id: i64) -> Result<Vec<Criterion>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT body FROM task_criteria WHERE task_id = ?1 ORDER BY position")?;
        let rows = statement.query_map(params![task_id], |row| row.get::<_, String>(0))?;
        rows.map(|row| {
            let body = row?;
            Criterion::new(&body).map_err(|_| Error::InvalidCriterion)
        })
        .collect()
    }
}

fn task_project_id(connection: &Connection, id: i64) -> Result<i64, Error> {
    connection
        .query_row(
            "SELECT project_id FROM tasks WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Error::UnknownTask { id },
            other => Error::Database(other),
        })
}

fn dependency_exists(
    connection: &Connection,
    task_id: i64,
    prerequisite_id: i64,
) -> Result<bool, Error> {
    match connection.query_row(
        "SELECT 1 FROM task_dependencies WHERE task_id = ?1 AND prerequisite_id = ?2",
        params![task_id, prerequisite_id],
        |_| Ok(()),
    ) {
        Ok(()) => Ok(true),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(error) => Err(Error::Database(error)),
    }
}

fn reaches(connection: &Connection, start: i64, target: i64) -> Result<bool, Error> {
    let mut statement =
        connection.prepare("SELECT task_id, prerequisite_id FROM task_dependencies")?;
    let edges: Vec<(i64, i64)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut adjacency: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
    for (task, prerequisite) in edges {
        adjacency.entry(task).or_default().push(prerequisite);
    }
    let mut visited: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut stack = vec![start];
    while let Some(current) = stack.pop() {
        if current == target {
            return Ok(true);
        }
        if !visited.insert(current) {
            continue;
        }
        if let Some(next) = adjacency.get(&current) {
            for id in next {
                if !visited.contains(id) {
                    stack.push(*id);
                }
            }
        }
    }
    Ok(false)
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
