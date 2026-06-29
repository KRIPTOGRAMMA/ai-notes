CREATE TABLE IF NOT EXISTS notes (
    id TEXT NOT NULL PRIMARY KEY,
    title TEXT NOT NULL DEFAULT 'Без названия',
    content TEXT NOT NULL DEFAULT '',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);
