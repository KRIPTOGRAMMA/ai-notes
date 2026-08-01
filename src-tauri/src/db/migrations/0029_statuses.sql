-- Task statuses: the ids are the former TaskStatus enum variants
-- (Todo/InProgress/Done/Archived), so existing tasks.status values stay valid
-- with no data migration — the same trick categories (0015) used for Category.
-- The original four carry is_reserved=1 and can be neither renamed nor deleted:
-- business logic is tied to them (Done -> hidden+completed_at, InProgress -> time
-- tracking, and several SQL queries compare against these strings directly).
CREATE TABLE statuses (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#888888',
    position INTEGER NOT NULL DEFAULT 0,
    is_reserved INTEGER NOT NULL DEFAULT 0
);

INSERT INTO statuses (id, name, color, position, is_reserved) VALUES
    ('Todo',       'Todo',      '#94a3b8', 0, 1),
    ('InProgress', 'В работе',  '#2a78d6', 1, 1),
    ('Done',       'Готово',    '#1baf7a', 2, 1),
    ('Archived',   'Архив',     '#6b7280', 3, 1);
