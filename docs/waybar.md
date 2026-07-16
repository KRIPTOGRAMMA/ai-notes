# AI Notes в waybar

`ai-notes --status` — короткоживущий CLI: читает БД приложения read-only,
печатает одну строку JSON в формате waybar custom module и выходит. Работает
и когда приложение запущено (WAL), и когда закрыто. Если БД ещё нет, выводит
пустой `text` — модуль просто ничего не показывает.

## Что показывается

Приоритет поля `text`:

1. `▶ Название до 14:30` — идёт тайм-блок (класс `block`)
2. `⏱ 15:00 Название` — следующий блок сегодня (класс `next`)
3. `▶ Название` — задача InProgress (класс `task`)
4. `☑ 3` — задачи с дедлайном на сегодня, включая просроченные (класс `due`)
5. `✓` — ничего не запланировано (класс `idle`)

В `tooltip` — детали: идущий/следующий блок, задача в работе, счётчик дедлайнов
с просрочкой, режим работы, пауза уведомлений. В `alt` — режим работы
(`Light` | `Study` | `Focus`), удобно для `format-icons`.

## Конфиг

`~/.config/waybar/config.jsonc`:

```jsonc
"custom/ai-notes": {
    "exec": "ai-notes --status",
    "return-type": "json",
    "interval": 30,
    "format": "{}",
    "max-length": 40,
    "on-click": "ai-notes",           // клик — открыть/показать приложение
    "on-click-right": "ai-notes -q"   // правый клик — быстрый ввод задачи
},
```

И добавить `"custom/ai-notes"` в `modules-left`/`modules-center`/`modules-right`.

`~/.config/waybar/style.css` (по классам из `class`):

```css
#custom-ai-notes.block { color: @green; }
#custom-ai-notes.next { color: @blue; }
#custom-ai-notes.due { color: @yellow; }
#custom-ai-notes.idle { opacity: 0.6; }
```

Если `ai-notes` не в `$PATH`, укажите полный путь к бинарю в `exec`.

## Другие панели

Формат совместим с любым баром, умеющим custom-скрипты: для polybar/i3blocks
можно взять `text` через `jq`:

```sh
ai-notes --status | jq -r .text
```

На системах без панели (или на Windows, где статус закрывает трей) модуль
просто не настраивается — сам CLI безвреден везде.
