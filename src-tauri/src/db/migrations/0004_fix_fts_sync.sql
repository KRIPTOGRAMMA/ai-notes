-- A bug in 0001/0002: the tasks_fts sync triggers used `id` (a TEXT UUID) as if
-- it were the rowid, whereas content_rowid='rowid' requires tasks.rowid itself.
-- The result: after the very first UPDATE the index diverges from the table and
-- MATCH over the new data fails with "database disk image is malformed".
-- Confirmed by hand.
--
-- The fix: recreate the virtual table and the triggers against rowid, then
-- rebuild the index from the current data.

DROP TRIGGER IF EXISTS tasks_ai;
DROP TRIGGER IF EXISTS tasks_au;
DROP TRIGGER IF EXISTS tasks_ad;
DROP TABLE IF EXISTS tasks_fts;

CREATE VIRTUAL TABLE tasks_fts USING fts5(
    title,
    description,
    tags,
    content='tasks',
    content_rowid='rowid'
);

CREATE TRIGGER tasks_ai AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts(rowid, title, description, tags)
    VALUES (new.rowid, new.title, new.description, new.tags);
END;

CREATE TRIGGER tasks_au AFTER UPDATE ON tasks BEGIN
    INSERT INTO tasks_fts(tasks_fts, rowid, title, description, tags)
    VALUES ('delete', old.rowid, old.title, old.description, old.tags);
    INSERT INTO tasks_fts(rowid, title, description, tags)
    VALUES (new.rowid, new.title, new.description, new.tags);
END;

CREATE TRIGGER tasks_ad AFTER DELETE ON tasks BEGIN
    INSERT INTO tasks_fts(tasks_fts, rowid, title, description, tags)
    VALUES ('delete', old.rowid, old.title, old.description, old.tags);
END;

-- Rebuild the index from the current state of tasks, in case the user already has
-- data indexed by the broken triggers.
INSERT INTO tasks_fts(rowid, title, description, tags)
SELECT rowid, title, description, tags FROM tasks;

-- id was neither a PRIMARY KEY nor unique, so WHERE id = ? did a full scan and
-- nothing prevented inserting a duplicate id.
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_id ON tasks(id);
