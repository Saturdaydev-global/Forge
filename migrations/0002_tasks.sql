CREATE TABLE tasks (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects (id),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    status TEXT NOT NULL DEFAULT 'backlog' CHECK (status = 'backlog')
);
CREATE TABLE task_criteria (
    task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 200),
    PRIMARY KEY (task_id, position)
);
CREATE INDEX index_tasks_project_id ON tasks (project_id);
