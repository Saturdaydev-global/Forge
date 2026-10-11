CREATE TABLE tasks_new (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects (id),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    status TEXT NOT NULL DEFAULT 'backlog'
        CHECK (status IN ('backlog','ready','running','review','completed','blocked','failed','cancelled'))
);
INSERT INTO tasks_new SELECT * FROM tasks;
CREATE TEMP TABLE saved_criteria AS SELECT * FROM task_criteria;
CREATE TEMP TABLE saved_dependencies AS SELECT * FROM task_dependencies;
DROP TABLE task_dependencies;
DROP TABLE task_criteria;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;
CREATE INDEX index_tasks_project_id ON tasks (project_id);
CREATE TABLE task_criteria (
    task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 200),
    PRIMARY KEY (task_id, position)
);
INSERT INTO task_criteria SELECT * FROM saved_criteria;
CREATE TABLE task_dependencies (
    task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    prerequisite_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, prerequisite_id),
    CHECK (task_id != prerequisite_id)
);
INSERT INTO task_dependencies SELECT * FROM saved_dependencies;
CREATE INDEX index_task_dependencies_prerequisite ON task_dependencies (prerequisite_id);
DROP TABLE saved_criteria;
DROP TABLE saved_dependencies;
CREATE TABLE runs (
    id INTEGER PRIMARY KEY,
    task_id INTEGER NOT NULL REFERENCES tasks (id),
    backend TEXT NOT NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running','succeeded','failed','cancelled','interrupted','timed_out')),
    started_at INTEGER NOT NULL DEFAULT (unixepoch()),
    finished_at INTEGER,
    cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0,1))
);
CREATE UNIQUE INDEX one_active_run_per_task ON runs(task_id) WHERE status='running';
CREATE TABLE run_events (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES runs (id),
    recorded_at INTEGER NOT NULL DEFAULT (unixepoch()),
    kind TEXT NOT NULL,
    detail TEXT NOT NULL
);
CREATE INDEX index_run_events_run ON run_events(run_id, id);
