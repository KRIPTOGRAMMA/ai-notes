CREATE TABLE IF NOT EXISTS pomodoro_log (
    id TEXT PRIMARY KEY,
    finished_at TEXT NOT NULL,
    task_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_pomodoro_log_finished_at ON pomodoro_log(finished_at);
