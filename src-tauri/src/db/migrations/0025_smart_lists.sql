-- Smart lists: saved user-defined task filters. The built-in lists ("Overdue",
-- "This week") are not stored in the DB — their predicate is fixed and computed
-- on the frontend; only user-defined ones live here, described by a filter over
-- category, priority, tag and whether a deadline is set.
CREATE TABLE IF NOT EXISTS smart_lists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    filter_json TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);
