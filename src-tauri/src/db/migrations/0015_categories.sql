-- Пользовательские категории задач вместо фиксированного enum.
-- id — стабильный ключ, который хранится в tasks.category (для посевных —
-- прежние значения enum, чтобы существующие задачи не трогать; для новых — uuid).
-- name/color — отображение, редактируются пользователем.
-- 'Other' — системный фолбэк: не удаляется, на него переназначаются задачи
-- удалённых категорий и невалидные значения при записи.
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
