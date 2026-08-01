-- Subtasks as a checklist inside a task. A task's progress is done / total, and
-- the link to the parent is task_id. No hard FK is declared (enforcement is off
-- by default in SQLite) — integrity is maintained in code: delete_task clears its
-- own subtasks.
CREATE TABLE IF NOT EXISTS subtasks (
    id          TEXT NOT NULL PRIMARY KEY,
    task_id     TEXT NOT NULL,
    title       TEXT NOT NULL,
    done        INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_subtasks_task ON subtasks(task_id);
