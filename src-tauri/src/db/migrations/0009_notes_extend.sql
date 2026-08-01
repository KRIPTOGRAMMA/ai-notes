-- Notes: tags (a JSON array of strings, as in tasks) and an optional link to a
-- task. No hard FK is declared (enforcement is off by default in SQLite) —
-- integrity is maintained in code: delete_task nulls linked_task_id.
ALTER TABLE notes ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
ALTER TABLE notes ADD COLUMN linked_task_id TEXT;
