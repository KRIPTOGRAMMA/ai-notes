-- Ручной порядок задач. Бэкфилл по rowid = прежний фактический порядок
-- (get_tasks без ORDER BY отдавал именно его). Новые задачи получают max+1.
ALTER TABLE tasks ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
UPDATE tasks SET sort_order = rowid;
