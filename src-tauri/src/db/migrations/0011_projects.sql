-- v0.5 фаза 2: проекты. Задачи и заметки могут принадлежать проекту.
-- FK в SQLite не enforced (как и везде в проекте) — целостность чистим руками
-- при удалении проекта.
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
