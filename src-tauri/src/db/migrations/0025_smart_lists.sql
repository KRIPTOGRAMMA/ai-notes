-- Умные списки (v0.9.14): сохранённые пользовательские фильтры задач.
-- Встроенные списки («Просроченные», «На этой неделе») в БД не хранятся —
-- их предикат фиксирован и вычисляется на фронте; здесь только свои,
-- определяемые фильтром по категории/приоритету/тегу/наличию дедлайна.
CREATE TABLE IF NOT EXISTS smart_lists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    filter_json TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);
