-- Projects. Tasks and notes may belong to a project. FKs are not enforced in
-- SQLite (as everywhere in this project), so integrity is cleaned up by hand when
-- a project is deleted.
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '',
    target_date DATETIME,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL
);

ALTER TABLE tasks ADD COLUMN project_id TEXT;
ALTER TABLE notes ADD COLUMN project_id TEXT;
