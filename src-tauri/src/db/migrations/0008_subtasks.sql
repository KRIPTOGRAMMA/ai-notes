-- Подзадачи как чек-лист внутри задачи (§4.1 / §5.1 ТЗ).
-- Прогресс задачи = done / total. Привязка к родителю через task_id.
-- FK не объявляем жёстко (в SQLite enforcement off по умолчанию) — целостность
-- держим в коде: delete_task чистит свои подзадачи.
CREATE TABLE IF NOT EXISTS subtasks (
    id          TEXT NOT NULL PRIMARY KEY,
    task_id     TEXT NOT NULL,
    title       TEXT NOT NULL,
    done        INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_subtasks_task ON subtasks(task_id);
