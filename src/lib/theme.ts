// Единый источник применения темы. Тема хранится в БД (AppSettings), но чтобы
// главный экран не мигал до загрузки настроек, дублируем последнее применённое
// значение в localStorage и применяем его синхронно на старте.

export type ThemeMode = "light" | "dark" | "system";

export interface ThemeColors {
  color_accent: string;
  color_accent_secondary: string;
  color_bg: string;
  color_text: string;
  color_border: string;
}

const LS_MODE = "theme_mode";
const LS_COLORS = "theme_colors";

// Осветление hex-цвета для --accent-hover (кастомный акцент не имеет своего hover).
function lighten(hex: string, amount = 0.15): string {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  const r = Math.min(255, Math.round(((n >> 16) & 0xff) + 255 * amount));
  const g = Math.min(255, Math.round(((n >> 8) & 0xff) + 255 * amount));
  const b = Math.min(255, Math.round((n & 0xff) + 255 * amount));
  return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, "0")}`;
}

let mql: MediaQueryList | null = null;
let mqlListener: ((e: MediaQueryListEvent) => void) | null = null;
let currentMode: ThemeMode = "system";

function applyDarkClass(mode: ThemeMode) {
  if (typeof document === "undefined") return;
  const dark = mode === "dark" || (mode === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

function applyColors(colors: Partial<ThemeColors>) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const set = (name: string, value: string | undefined) => {
    if (value && value.trim()) root.style.setProperty(name, value.trim());
    else root.style.removeProperty(name);
  };
  set("--accent", colors.color_accent);
  if (colors.color_accent && colors.color_accent.trim()) {
    root.style.setProperty("--accent-hover", lighten(colors.color_accent));
  } else {
    root.style.removeProperty("--accent-hover");
  }
  // Второй акцент — пусто = равен первому (градиент на .btn-primary вырождается в сплошной цвет).
  set("--accent-secondary", colors.color_accent_secondary?.trim() ? colors.color_accent_secondary : colors.color_accent);
  set("--bg-primary", colors.color_bg);
  set("--text-primary", colors.color_text);
  set("--border", colors.color_border);
}

// Применяет тему и кеширует её в localStorage. Для system подписывается на смену
// системной темы (переустанавливая слушатель, чтобы не копить дубликаты).
export function applyTheme(mode: ThemeMode, colors: Partial<ThemeColors>) {
  currentMode = mode;
  applyDarkClass(mode);
  applyColors(colors);

  if (typeof window !== "undefined") {
    if (!mql) mql = window.matchMedia("(prefers-color-scheme: dark)");
    if (mqlListener) mql.removeEventListener("change", mqlListener);
    if (mode === "system") {
      mqlListener = () => applyDarkClass(currentMode);
      mql.addEventListener("change", mqlListener);
    } else {
      mqlListener = null;
    }
  }

  try {
    localStorage.setItem(LS_MODE, mode);
    localStorage.setItem(LS_COLORS, JSON.stringify(colors ?? {}));
  } catch {
    // приватный режим / недоступный localStorage — не критично
  }
}

// Синхронное применение из кеша до загрузки настроек из БД (анти-мигание).
export function applyCachedTheme() {
  try {
    const mode = (localStorage.getItem(LS_MODE) as ThemeMode | null) ?? "system";
    const colors = JSON.parse(localStorage.getItem(LS_COLORS) ?? "{}") as Partial<ThemeColors>;
    applyTheme(mode, colors);
  } catch {
    applyTheme("system", {});
  }
}
