// Deciding what the backup section of Settings should say.
//
// A pure module for the same reason as guard.ts, listnav.ts and uistate.ts:
// vitest runs without the Svelte plugin (vitest.config.ts), so logic living
// inside a .svelte file cannot be unit-tested at all. The decision lives here;
// the component only maps the level onto a translated string.
//
// Why this exists (v0.9.85): Settings.svelte declared `lastBackup` and rendered
// it behind an {#if}, but never assigned it — the "last backup" line had never
// appeared for anyone. Meanwhile last_auto_backup was written by the backend and
// never added to AppSettings, so it never crossed the boundary. The user could
// not tell "backups are running" from "no folder set" from "the backup task died
// months ago". All three now map onto a distinct level.

export type BackupLevel = "error" | "off" | "pending" | "stale" | "ok";

// Two missed daily cycles. One missed run is normal — the machine was off, or
// the 24h gate landed just after a launch — and warning about it would train the
// user to ignore the warning.
const STALE_AFTER_HOURS = 48;

export function backupHealth(
  dir: string,
  last: string,
  now: Date,
  error?: string,
): BackupLevel {
  // An error outranks everything: the folder is set and a run was attempted, so
  // "off" or a stale date would both understate the problem.
  if (error && error.trim() !== "") return "error";
  if (dir.trim() === "") return "off";
  if (last.trim() === "") return "pending";

  const at = new Date(last);
  // An unparseable timestamp is not proof of a recent backup, so it must not
  // read as "ok" — treat it the same as never having run.
  if (Number.isNaN(at.getTime())) return "pending";

  const hours = (now.getTime() - at.getTime()) / 3_600_000;
  return hours > STALE_AFTER_HOURS ? "stale" : "ok";
}
