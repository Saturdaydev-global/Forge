CREATE TABLE task_dependencies (
    task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    prerequisite_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, prerequisite_id),
    CHECK (task_id != prerequisite_id)
);
CREATE INDEX index_task_dependencies_prerequisite ON task_dependencies (prerequisite_id);
