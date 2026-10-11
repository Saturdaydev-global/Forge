use std::{
    fs::{File, OpenOptions},
    path::Path,
    thread,
    time::{Duration, Instant},
};

use rusqlite::{Connection, TransactionBehavior, params};

use crate::{Error, domain::TaskStatus, storage::Store};

#[derive(Debug)]
pub struct Run {
    pub id: i64,
    pub task_id: i64,
    pub backend: String,
    pub model: String,
    pub status: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
    TimedOut,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::TimedOut => "timed_out",
        }
    }
}

// Keep this file open for the worker lifetime; OS locks are released after a crash.
pub struct WorkerGuard(File, std::path::PathBuf);

impl WorkerGuard {
    fn check(&self, connection: &Connection) -> Result<(), Error> {
        let path = connection.path().ok_or(Error::WorkerBusy)?;
        if Path::new(path).canonicalize()? != self.1 {
            return Err(Error::WorkerBusy);
        }
        Ok(())
    }
    pub fn acquire(db: &Path) -> Result<Self, Error> {
        let db = db.canonicalize()?;
        let mut name = db.as_os_str().to_os_string();
        name.push(".worker-lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(name)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => Error::WorkerBusy,
            std::fs::TryLockError::Error(error) => Error::Io(error),
        })?;
        Ok(Self(file, db))
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

impl Store {
    pub fn change_status(&mut self, id: i64, next: TaskStatus) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = status(&tx, id)?;
        if !current.can_change_to(next) {
            return Err(Error::InvalidTransition {
                from: current.as_str().into(),
                to: next.as_str().into(),
            });
        }
        if next == TaskStatus::Completed {
            require_dependencies(&tx, id)?;
        }
        tx.execute(
            "UPDATE tasks SET status=?2 WHERE id=?1",
            params![id, next.as_str()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn ready_tasks(&self, project_id: i64) -> Result<Vec<i64>, Error> {
        self.list_tasks(project_id)?;
        let mut statement = self.connection.prepare("SELECT t.id FROM tasks t WHERE project_id=?1 AND status='ready' AND NOT EXISTS (SELECT 1 FROM task_dependencies d JOIN tasks p ON p.id=d.prerequisite_id WHERE d.task_id=t.id AND p.status!='completed') ORDER BY t.id")?;
        Ok(statement
            .query_map([project_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn begin_run(
        &mut self,
        task_id: i64,
        backend: &str,
        model: &str,
        guard: &WorkerGuard,
    ) -> Result<i64, Error> {
        guard.check(&self.connection)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = status(&tx, task_id)?;
        if current != TaskStatus::Ready {
            return Err(Error::InvalidTransition {
                from: current.as_str().into(),
                to: "running".into(),
            });
        }
        require_dependencies(&tx, task_id)?;
        tx.execute(
            "INSERT INTO runs(task_id,backend,model,status) VALUES (?1,?2,?3,'running')",
            params![task_id, backend, model],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("UPDATE tasks SET status='running' WHERE id=?1", [task_id])?;
        event(
            &tx,
            id,
            "started",
            "Execution started; acceptance remains subject to review",
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn finish_run(&mut self, id: i64, mut outcome: Outcome) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (task_id, cancel): (i64, bool) = tx
            .query_row(
                "SELECT task_id,cancel_requested FROM runs WHERE id=?1 AND status='running'",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| {
                if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                    Error::InactiveRun(id)
                } else {
                    e.into()
                }
            })?;
        if cancel && outcome != Outcome::Interrupted {
            outcome = Outcome::Cancelled;
        }
        let next = match outcome {
            Outcome::Succeeded => "review",
            Outcome::Cancelled => "cancelled",
            _ => "failed",
        };
        tx.execute(
            "UPDATE runs SET status=?2,finished_at=unixepoch() WHERE id=?1",
            params![id, outcome.as_str()],
        )?;
        tx.execute(
            "UPDATE tasks SET status=?2 WHERE id=?1",
            params![task_id, next],
        )?;
        event(&tx, id, outcome.as_str(), "Execution finished")?;
        tx.commit()?;
        Ok(())
    }

    pub fn request_cancel(&mut self, id: i64) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute(
            "UPDATE runs SET cancel_requested=1 WHERE id=?1 AND status='running'",
            [id],
        )? == 0
        {
            return Err(Error::InactiveRun(id));
        }
        event(&tx, id, "cancel_requested", "User requested cancellation")?;
        tx.commit()?;
        Ok(())
    }

    pub fn cancellation_requested(&self, id: i64) -> Result<bool, Error> {
        Ok(self.connection.query_row(
            "SELECT cancel_requested FROM runs WHERE id=?1 AND status='running'",
            [id],
            |r| r.get(0),
        )?)
    }

    pub fn runs(&self, task_id: i64) -> Result<Vec<Run>, Error> {
        self.get_task(task_id)?;
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, backend, model, status FROM runs WHERE task_id=?1 ORDER BY id",
        )?;
        Ok(statement
            .query_map([task_id], |r| {
                Ok(Run {
                    id: r.get(0)?,
                    task_id: r.get(1)?,
                    backend: r.get(2)?,
                    model: r.get(3)?,
                    status: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn events(&self, run_id: i64) -> Result<Vec<(String, String)>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT kind,detail FROM run_events WHERE run_id=?1 ORDER BY id")?;
        Ok(statement
            .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn recover(&mut self, guard: &WorkerGuard) -> Result<usize, Error> {
        guard.check(&self.connection)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO run_events(run_id,kind,detail) SELECT id,'interrupted','Worker stopped; no automatic replay' FROM runs WHERE status='running'", [])?;
        tx.execute("UPDATE tasks SET status='failed' WHERE id IN (SELECT task_id FROM runs WHERE status='running')", [])?;
        let count = tx.execute(
            "UPDATE runs SET status='interrupted',finished_at=unixepoch() WHERE status='running'",
            [],
        )?;
        tx.commit()?;
        Ok(count)
    }
}

pub fn run_mock(db: &Path, task_id: i64, fail: bool, delay_ms: u64) -> Result<i64, Error> {
    let mut store = Store::open(db)?;
    let guard = WorkerGuard::acquire(db)?;
    let id = store.begin_run(task_id, "mock", "simulated", &guard)?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(delay_ms) {
        if store.cancellation_requested(id)? {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    store.finish_run(
        id,
        if fail {
            Outcome::Failed
        } else {
            Outcome::Succeeded
        },
    )?;
    Ok(id)
}

fn status(connection: &Connection, id: i64) -> Result<TaskStatus, Error> {
    let value: String = connection
        .query_row("SELECT status FROM tasks WHERE id=?1", [id], |r| r.get(0))
        .map_err(|e| {
            if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                Error::UnknownTask { id }
            } else {
                e.into()
            }
        })?;
    value.parse()
}

fn require_dependencies(connection: &Connection, id: i64) -> Result<(), Error> {
    let incomplete: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM task_dependencies d JOIN tasks p ON p.id=d.prerequisite_id WHERE d.task_id=?1 AND p.status!='completed')", [id], |r| r.get(0))?;
    if incomplete {
        return Err(Error::IncompleteDependencies(id));
    }
    Ok(())
}

fn event(connection: &Connection, id: i64, kind: &str, detail: &str) -> Result<(), Error> {
    connection.execute(
        "INSERT INTO run_events(run_id,kind,detail) VALUES (?1,?2,?3)",
        params![id, kind, detail],
    )?;
    Ok(())
}
