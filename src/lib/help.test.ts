import { describe, it, expect } from "vitest";
import { HELP_TOPICS } from "./help";

describe("HELP_TOPICS", () => {
  it("id тем уникальны — используются как ключи {#each}", () => {
    const ids = HELP_TOPICS.map(t => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("все темы и пункты непусты", () => {
    expect(HELP_TOPICS.length).toBeGreaterThan(0);
    for (const topic of HELP_TOPICS) {
      expect(topic.title.trim()).not.toBe("");
      expect(topic.items.length).toBeGreaterThan(0);
      for (const item of topic.items) {
        expect(item.term.trim()).not.toBe("");
        expect(item.desc.trim()).not.toBe("");
      }
    }
  });

  // Главный риск справки — молчаливое устаревание. Переназначаемые хоткеи
  // живут данными в keybinds.ts и рендерятся на вкладке «Хоткеи» с ТЕКУЩИМИ
  // комбинациями; продублировать их здесь текстом — значит начать врать при
  // первом же переназначении. Поэтому справка пишет, ГДЕ смотреть, а не какие.
  //
  // Комбинации внутри полей ввода (Ctrl+Enter, Shift+Enter, Ctrl+V, Ctrl+Tab,
  // Ctrl+клик) — другое дело: они зашиты в обработчиках, в keybinds.ts их нет,
  // переназначить их нельзя, поэтому устареть они не могут.
  it("не дублирует переназначаемые хоткеи — их значения только в keybinds.ts", () => {
    const text = HELP_TOPICS
      .flatMap(t => t.items.map(i => `${i.term} ${i.desc}`))
      .join(" ");
    // Комбинации навигации и палитры (Ctrl+K, Ctrl+D, Ctrl+1..7)
    expect(text).not.toMatch(/Ctrl\s*\+\s*[KDkd]\b/);
    expect(text).not.toMatch(/Ctrl\s*\+\s*\d/);
    // Глобальные хоткеи быстрого ввода (Ctrl+Shift+N/M/B)
    expect(text).not.toMatch(/Ctrl\s*\+?\s*Shift\s*\+?\s*[NMBnmb]\b/);
  });

  // Пути зависят от ОС и от identifier'а приложения — в v0.9.28 зашитая
  // строка `~/.local/share/ai-notes/...` оказалась неверной на всех ОС.
  it("не содержит захардкоженных путей", () => {
    const text = HELP_TOPICS
      .flatMap(t => t.items.map(i => i.desc))
      .join(" ");
    expect(text).not.toContain(".local/share");
    expect(text).not.toContain("%APPDATA%");
    expect(text).not.toContain("Library/Application Support");
  });
});
