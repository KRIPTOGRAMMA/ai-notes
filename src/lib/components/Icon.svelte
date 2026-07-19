<script lang="ts">
  // Инлайн-SVG иконки вместо emoji (v0.8.1): emoji-глифы (🍅 ▶ ⏸ 📅 …)
  // рендерятся на Linux непредсказуемо — зависят от установленных
  // emoji-шрифтов. Тот же паттерн, что sidebar-навигация в App.svelte:
  // stroke="currentColor", без внешних шрифтов/CDN.
  const PATHS: Record<string, string> = {
    play:     "M7 4.5v15l13-7.5Z",
    stop:     "M6.5 6.5h11v11h-11Z",
    pause:    "M9 5v14 M15 5v14",
    skip:     "M5 5v14l9-7Z M18 5v14",
    calendar: "M5 5h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z M16 3v4 M8 3v4 M3 11h18",
    link:     "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71 M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
    sparkles: "M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9Z M19 15l.9 2.1L22 18l-2.1.9L19 21l-.9-2.1L16 18l2.1-.9Z",
    clock:    "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z M12 7v5l3 2",
    zap:      "M13 2 3 14h9l-1 8 10-12h-9l1-8Z",
    shuffle:  "M16 3h5v5 M4 20 21 3 M21 16v5h-5 M15 15l6 6 M4 4l5 5",
    pencil:   "M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z",
    coffee:   "M17 8h1a4 4 0 1 1 0 8h-1 M3 8h14v9a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4Z",
    timer:    "M10 2h4 M12 14l3-3 M12 22a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z",
    tag:      "M20.6 13.4 12 22 2 12V2h10l8.6 8.6a2 2 0 0 1 0 2.8Z M7 7h.01",
    flag:     "M4 21V3 M4 4h12l-2 4 2 4H4",
    target:   "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z M12 18a6 6 0 1 0 0-12 6 6 0 0 0 0 12Z M12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z",
    lock:     "M5 11h14a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1Z M8 11V7a4 4 0 0 1 8 0v4",
    alert:    "M12 3 2 20h20Z M12 9v5 M12 17h.01",
  };

  let { name, size = 14 }: { name: string; size?: number } = $props();
</script>

<svg viewBox="0 0 24 24" width={size} height={size} fill="none"
  stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"
  style="vertical-align:-0.125em;flex-shrink:0;">
  <path d={PATHS[name] ?? ""} />
</svg>
