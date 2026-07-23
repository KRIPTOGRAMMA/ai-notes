-- Статусы задач (v0.9.20): те же id, что раньше были вариантами enum
-- TaskStatus (Todo/InProgress/Done/Archived), чтобы существующие значения
-- tasks.status остались валидными без миграции данных — тот же приём,
-- что categories (0015) сделала для Category. is_reserved=1 у исходных
-- четырёх — не переименовываются/не удаляются: с ними жёстко завязана
-- бизнес-логика (Done → hidden+completed_at, InProgress → тайм-трекинг,
-- несколько SQL-запросов сравнивают напрямую со строками этих статусов).
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
