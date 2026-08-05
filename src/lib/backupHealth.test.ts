import { describe, it, expect } from "vitest";
import { backupHealth } from "./backupHealth";

const NOW = new Date("2026-08-05T12:00:00Z");
const hoursAgo = (h: number) => new Date(NOW.getTime() - h * 3_600_000).toISOString();

describe("backupHealth", () => {
  // The state the real database was actually in when this was written: the
  // folder was never set, so auto_backup_due always returned false and no
  // backup had ever run. It has to be distinguishable from "ran a while ago".
  it("пустая папка — бэкап выключен", () => {
    expect(backupHealth("", hoursAgo(1), NOW)).toBe("off");
    expect(backupHealth("   ", "", NOW)).toBe("off");
  });

  it("папка есть, копий ещё не было — ожидание, а не ошибка", () => {
    expect(backupHealth("/backups", "", NOW)).toBe("pending");
  });

  it("свежая копия — всё в порядке", () => {
    expect(backupHealth("/backups", hoursAgo(1), NOW)).toBe("ok");
    expect(backupHealth("/backups", hoursAgo(47), NOW)).toBe("ok");
  });

  // Two missed daily cycles. 47h stays "ok" above so a single skipped run — the
  // machine was off overnight — does not cry wolf.
  it("старше 48 часов — устарел", () => {
    expect(backupHealth("/backups", hoursAgo(49), NOW)).toBe("stale");
  });

  // The backup loop records its failure separately; a failed run leaves the old
  // (still valid, still recent) timestamp in place, so without this branch a
  // broken backup would render as "ok".
  it("записанная ошибка перевешивает свежую дату", () => {
    expect(backupHealth("/backups", hoursAgo(1), NOW, "2026-08-05\tno space left")).toBe("error");
    expect(backupHealth("", "", NOW, "2026-08-05\tboom")).toBe("error");
  });

  it("пустая строка ошибки ошибкой не считается", () => {
    expect(backupHealth("/backups", hoursAgo(1), NOW, "   ")).toBe("ok");
  });

  // A corrupt timestamp must not read as a successful recent backup.
  it("нечитаемая дата не выдаётся за успешную копию", () => {
    expect(backupHealth("/backups", "не дата", NOW)).toBe("pending");
  });
});
