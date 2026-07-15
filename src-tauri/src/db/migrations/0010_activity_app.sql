-- v0.5 фаза 1: трекинг по приложениям. Каждая Active-строка лога знает класс
-- окна в фокусе на момент тика (NULL — провайдер окон недоступен или Idle).
ALTER TABLE activity_log ADD COLUMN app TEXT;
