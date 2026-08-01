-- User-defined task categories in place of a fixed enum.
-- id is the stable key stored in tasks.category (for the seeded rows these are
-- the former enum values, so existing tasks are left untouched; new ones get a
-- uuid). name and color are presentation and are edited by the user.
-- 'Other' is the system fallback: it cannot be deleted, and tasks from deleted
-- categories as well as invalid values on write are reassigned to it.
CREATE TABLE categories (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#888888',
    position INTEGER NOT NULL DEFAULT 0
);

INSERT INTO categories (id, name, color, position) VALUES
    ('Work',   'Работа',   '#2a78d6', 0),
    ('Study',  'Учёба',    '#1baf7a', 1),
    ('Home',   'Дом',      '#eda100', 2),
    ('Health', 'Здоровье', '#008300', 3),
    ('Other',  'Другое',   '#4a3aa7', 4);
