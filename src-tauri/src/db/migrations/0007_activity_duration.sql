-- Каждая строка activity_log теперь знает свою длительность:
-- интервал тика настраиваемый, поэтому "COUNT(*) * константа" больше не работает.
-- Legacy-строки писались с тиком 60 секунд — оставляем DEFAULT 60.
ALTER TABLE activity_log ADD COLUMN duration_secs INTEGER NOT NULL DEFAULT 60;
