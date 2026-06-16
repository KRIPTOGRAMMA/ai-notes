CREATE VIRTUAL TABLE IF NOT EXISTS tasks_fts USING fts5(
    id UNINDEXED,
    title,
    description,
    tags,
    content='tasks',
    content_rowid='rowid'
);

-- Триггеры для синхронизации с tasks
CREATE TRIGGER IF NOT EXISTS tasks_ai AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts(id, title, description, tags)
    VALUES (new.id, new.title, new.description, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS tasks_au AFTER UPDATE ON tasks BEGIN
    INSERT INTO tasks_fts(tasks_fts, id, title, description, tags)
    VALUES ('delete', old.id, old.title, old.description, old.tags);
    INSERT INTO tasks_fts(id, title, description, tags)
    VALUES (new.id, new.title, new.description, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS tasks_ad AFTER DELETE ON tasks BEGIN
    INSERT INTO tasks_fts(tasks_fts, id, title, description, tags)
    VALUES ('delete', old.id, old.title, old.description, old.tags);
END;