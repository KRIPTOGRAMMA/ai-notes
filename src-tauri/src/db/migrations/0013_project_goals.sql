-- Project goals: N tasks and/or N minutes of time blocks per period (week or
-- month). notified_goal holds the key of the period (the local date it started)
-- for which the completion push has already been sent — in a new period the goal
-- re-arms itself.
ALTER TABLE projects ADD COLUMN goal_tasks INTEGER;
ALTER TABLE projects ADD COLUMN goal_mins INTEGER;
ALTER TABLE projects ADD COLUMN goal_period TEXT NOT NULL DEFAULT 'week';
ALTER TABLE projects ADD COLUMN notified_goal TEXT NOT NULL DEFAULT '';
