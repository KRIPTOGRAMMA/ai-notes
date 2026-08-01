-- Зависимости задач (v0.9.56): «Б заблокирована задачей А».
--
-- Отдельная таблица, а не поле blocked_by в tasks: у задачи может быть
-- несколько блокеров, и разблокируется она только когда закрыты все.
-- Стоит столько же, сколько одиночное поле, но не упрётся позже.
--
-- ON DELETE CASCADE — про физическое удаление строки задачи. Корзина им не
-- пользуется: она мягкая (tasks.deleted_at), поэтому зависимость переживает
-- удаление блокера в Корзину и возвращается вместе с ним при восстановлении.
-- Разблокировку на время нахождения блокера в Корзине считает уже SQL в
-- commands/dependencies.rs, а не эта схема.
--
-- PRIMARY KEY (task_id, blocker_id) сам гасит дубли связи.
CREATE TABLE task_dependencies (
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    blocker_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (task_id, blocker_id)
);

-- Ходим в обе стороны: «кто блокирует меня» при отрисовке списка задач и
-- «кого разблокирует эта» при её выполнении.
CREATE INDEX idx_task_deps_task ON task_dependencies(task_id);
CREATE INDEX idx_task_deps_blocker ON task_dependencies(blocker_id);
