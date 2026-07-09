-- Заметки: теги (JSON-массив строк, как в tasks) и необязательная привязка к
-- задаче. FK не объявляем жёстко (в SQLite enforcement off по умолчанию) —
-- целостность держим в коде: delete_task обнуляет linked_task_id.
ALTER TABLE notes ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
ALTER TABLE notes ADD COLUMN linked_task_id TEXT;
