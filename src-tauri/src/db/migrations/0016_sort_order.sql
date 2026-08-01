-- Manual task ordering. The backfill by rowid matches the previous de facto order
-- (get_tasks without an ORDER BY returned exactly that). New tasks get max+1.
ALTER TABLE tasks ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
UPDATE tasks SET sort_order = rowid;
