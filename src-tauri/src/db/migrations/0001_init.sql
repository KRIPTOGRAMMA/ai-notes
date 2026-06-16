CREATE TABLE IF NOT EXISTS tasks (
    id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'Todo',
    priority TEXT NOT NULL DEFAULT 'Medium',
    category TEXT NOT NULL DEFAULT 'Other',
    deadline DATETIME,
    tags TEXT NOT NULL DEFAULT '[]',
    recurrence TEXT NOT NULL DEFAULT 'None',
    hidden INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    completed_at DATETIME,
    notified_24h INTEGER NOT NULL DEFAULT 0,
    notified_1h INTEGER NOT NULL DEFAULT 0,
    notified_deadline INTEGER NOT NULL DEFAULT 0
);