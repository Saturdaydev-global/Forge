CREATE TABLE models (
    id TEXT PRIMARY KEY,
    cost TEXT NOT NULL CHECK (cost IN ('free','subscription','paid','unknown')),
    access TEXT NOT NULL CHECK (access IN ('anonymous','subscription','api-key','unknown')),
    source TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0,1)),
    priority INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
