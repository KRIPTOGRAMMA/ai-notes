CREATE TABLE IF NOT EXISTS project_goal_history (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    period_key TEXT NOT NULL,
    goal_tasks INTEGER,
    goal_mins INTEGER,
    done_tasks INTEGER NOT NULL DEFAULT 0,
    done_mins INTEGER NOT NULL DEFAULT 0,
    recorded_at TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_goal_history_project_period
    ON project_goal_history(project_id, period_key);
