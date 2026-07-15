-- Цели проектов (v0.5 фаза 2, отложенная часть): N задач и/или N минут
-- тайм-блоков за период (неделя/месяц). notified_goal хранит ключ периода
-- (локальная дата его начала), за который уже отправлен пуш о выполнении —
-- в новом периоде цель «перезаряжается».
ALTER TABLE projects ADD COLUMN goal_tasks INTEGER;
ALTER TABLE projects ADD COLUMN goal_mins INTEGER;
ALTER TABLE projects ADD COLUMN goal_period TEXT NOT NULL DEFAULT 'week';
ALTER TABLE projects ADD COLUMN notified_goal TEXT NOT NULL DEFAULT '';
