-- FTS-поиск по заметкам — тот же паттерн, что tasks_fts после фикса 0004:
-- external content на notes.rowid, синхронизация триггерами.
CREATE VIRTUAL TABLE notes_fts USING fts5(
    title,
    content,
    tags,
    content='notes',
    content_rowid='rowid'
);

CREATE TRIGGER notes_ai AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts(rowid, title, content, tags)
    VALUES (new.rowid, new.title, new.content, new.tags);
END;

CREATE TRIGGER notes_au AFTER UPDATE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, content, tags)
    VALUES ('delete', old.rowid, old.title, old.content, old.tags);
    INSERT INTO notes_fts(rowid, title, content, tags)
    VALUES (new.rowid, new.title, new.content, new.tags);
END;

CREATE TRIGGER notes_ad AFTER DELETE ON notes BEGIN
    INSERT INTO notes_fts(notes_fts, rowid, title, content, tags)
    VALUES ('delete', old.rowid, old.title, old.content, old.tags);
END;

-- Индексируем уже существующие заметки.
INSERT INTO notes_fts(rowid, title, content, tags)
SELECT rowid, title, content, tags FROM notes;
