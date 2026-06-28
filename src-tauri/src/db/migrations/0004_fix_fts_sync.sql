-- 0001/0002 баг: триггеры синхронизации tasks_fts использовали `id` (TEXT UUID)
-- как будто это rowid. content_rowid='rowid' требует именно tasks.rowid.
-- Итог: после первого же UPDATE индекс расходится с таблицей, а MATCH по новым
-- данным падает с "database disk image is malformed". Подтверждено вручную.
--
-- Чиним: пересоздаём виртуальную таблицу и триггеры на rowid, перестраиваем индекс
-- из текущих данных.

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

-- Перестраиваем индекс из текущего состояния tasks (на случай, если у пользователя
-- уже есть данные, проиндексированные сломанными триггерами).
INSERT INTO tasks_fts(rowid, title, description, tags)
SELECT rowid, title, description, tags FROM tasks;

-- id не было ни PRIMARY KEY, ни уникальным — WHERE id = ? делал full scan,
-- и ничего не мешало вставить дубликат id.
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_id ON tasks(id);
