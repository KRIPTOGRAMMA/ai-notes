import { describe, it, expect } from "vitest";
import { pad2, hhmm, hhmmFromMins, localDateKey, toLocalInput, duration, localeTag } from "./datetime";

describe("pad2", () => {
  it("дополняет до двух знаков", () => {
    expect(pad2(0)).toBe("00");
    expect(pad2(7)).toBe("07");
    expect(pad2(23)).toBe("23");
  });
});

describe("localDateKey", () => {
  // The bug this function exists to prevent: toISOString() gives the UTC day, so
  // just after local midnight it reports yesterday — the daily note opens under
  // the wrong name and the heatmap counts the task on the wrong square.
  it("отдаёт ЛОКАЛЬНЫЙ день, а не UTC", () => {
    const justAfterMidnight = new Date(2026, 0, 1, 0, 30);
    expect(localDateKey(justAfterMidnight)).toBe("2026-01-01");
  });

  it("месяц и день дополнены нулями", () => {
    expect(localDateKey(new Date(2026, 7, 3, 12, 0))).toBe("2026-08-03");
  });

  it("последний час суток остаётся тем же днём", () => {
    expect(localDateKey(new Date(2026, 11, 31, 23, 59))).toBe("2026-12-31");
  });
});

describe("hhmm", () => {
  it("дополняет обе половины", () => {
    expect(hhmm(new Date(2026, 0, 1, 9, 5))).toBe("09:05");
    expect(hhmm(new Date(2026, 0, 1, 0, 0))).toBe("00:00");
    expect(hhmm(new Date(2026, 0, 1, 23, 59))).toBe("23:59");
  });
});

describe("hhmmFromMins", () => {
  it("минуты от полуночи разворачиваются в часы:минуты", () => {
    expect(hhmmFromMins(0)).toBe("00:00");
    expect(hhmmFromMins(545)).toBe("09:05");
    expect(hhmmFromMins(1439)).toBe("23:59");
  });
});

describe("toLocalInput", () => {
  // datetime-local expects local time. Feeding it the UTC string shifts the
  // deadline by the timezone offset on every open-and-save.
  it("round-trip через Date не сдвигает время", () => {
    const d = new Date(2026, 7, 3, 9, 5);
    const s = toLocalInput(d.toISOString());
    expect(s).toBe("2026-08-03T09:05");
    const back = new Date(s);
    expect(back.getHours()).toBe(9);
    expect(back.getDate()).toBe(3);
  });
});

describe("duration", () => {
  it("до часа — минуты:секунды", () => {
    expect(duration(0)).toBe("0:00");
    expect(duration(65)).toBe("1:05");
    expect(duration(3599)).toBe("59:59");
  });

  it("от часа — часы:минуты:секунды", () => {
    expect(duration(3600)).toBe("1:00:00");
    expect(duration(3661)).toBe("1:01:01");
  });

  // The pomodoro widget has always shown "65:00" rather than "1:05:00"; the flag
  // keeps that behaviour instead of changing it silently.
  it("alwaysMinutes не переходит на часы", () => {
    expect(duration(3900, true)).toBe("65:00");
    expect(duration(65, true)).toBe("1:05");
  });

  it("отрицательное время не даёт мусора", () => {
    expect(duration(-5)).toBe("0:00");
  });
});

describe("localeTag", () => {
  it("язык интерфейса разворачивается в тег Intl", () => {
    expect(localeTag("en")).toBe("en-US");
    expect(localeTag("ru")).toBe("ru-RU");
  });
});
