<script lang="ts">
  // Чек-лист подзадач одним редактором (v0.9.45).
  //
  // Раньше каждая подзадача была своим <input>: стрелки не переводили курсор
  // между строками, выделить несколько строк было нельзя, вставка списка из
  // буфера давала одну подзадачу с переводами строк внутри.
  //
  // Первый заход этой версии свёл список в <textarea> с видимыми префиксами
  // `[x] `, а чекбоксы поставил колонкой сбоку. Разбор APK Xiaomi Notes
  // (com.miui.richeditor.style.CheckboxSpan) показал, что там сделано иначе и
  // разница принципиальная: чекбокс — часть строки, а не соседний столбец, а
  // разметка пользователю не видна вообще (в их формате она живёт только в
  // сериализации, `HtmlParser$CheckBoxElement` → `<input type="checkbox">`).
  // Видимые скобки пользователь может испортить, а колонка сбоку разъезжается
  // на любой строке, которая перенеслась.
  //
  // Поэтому здесь CodeMirror с тем же приёмом, что уже работает в заметках
  // (v0.9.27): `Decoration.replace` прячет `[x] ` и рисует на его месте
  // настоящий чекбокс внутри строки. Текст остаётся одним документом, поэтому
  // стрелки, выделение через строки, undo и вставка достаются от редактора.
  //
  // Берём голый CodeMirror, а не LiveMarkdownEditor: тому нужны вики-ссылки,
  // картинки, таблицы и автодополнение — здесь всё это лишнее.
  import { onMount, onDestroy } from "svelte";
  import { EditorState } from "@codemirror/state";
  import {
    EditorView, Decoration, type DecorationSet, WidgetType,
    keymap, ViewPlugin, type ViewUpdate, drawSelection, placeholder as cmPlaceholder,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { CHECK_RE, toggleLine, lineIndexAt } from "../checklistText";

  type Props = {
    value: string;
    placeholder?: string;
    // Текст отдаётся аргументом, а не только через bind:value — панель в
    // списке задач держит его в словаре по id задачи, и двусторонний бинд к
    // элементу словаря там неудобен.
    onchange?: (text: string) => void;
  };

  let { value = $bindable(""), placeholder = "", onchange }: Props = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;

  class CheckboxWidget extends WidgetType {
    checked: boolean;
    pos: number;
    constructor(checked: boolean, pos: number) {
      super();
      this.checked = checked;
      this.pos = pos;
    }
    eq(other: CheckboxWidget) {
      return other.checked === this.checked && other.pos === this.pos;
    }
    toDOM() {
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = this.checked;
      box.className = "cm-sub-checkbox";
      // Фокус остаётся в тексте: клик по галочке не должен выбивать каретку из
      // строки, которую пользователь правит.
      box.onmousedown = (e) => e.preventDefault();
      box.onclick = () => {
        if (!view) return;
        const doc = view.state.doc.toString();
        const index = lineIndexAt(doc, this.pos);
        const next = toggleLine(doc, index);
        if (next === doc) return;
        view.dispatch({ changes: { from: 0, to: doc.length, insert: next } });
      };
      return box;
    }
    ignoreEvent() { return false; }
  }

  // Разметку прячем всегда, в том числе на строке с курсором — в отличие от
  // заметок, где маркеры показываются под кареткой. Здесь скобки не часть
  // текста заметки, а способ хранения отметки: показывать их пользователю
  // незачем, а испортить он их может.
  //
  // Чекбокс рисуется у КАЖДОЙ непустой строки, а не только у размеченной.
  // Иначе получается рассинхрон: `parseChecklist` считает строку без префикса
  // подзадачей (это нужно для вставки готового списка), и при сохранении она
  // ею станет — а чекбокса у неё нет, и пользователь видит часть строк
  // подзадачами, а часть просто текстом. У неразмеченной строки виджет
  // вставляется в начало (`Decoration.widget`), а не заменяет текст.
  function buildDecos(state: EditorState): DecorationSet {
    const items: { from: number; to: number; deco: Decoration }[] = [];
    for (let n = 1; n <= state.doc.lines; n++) {
      const line = state.doc.line(n);
      const m = CHECK_RE.exec(line.text);
      if (m) {
        const from = line.from + m[1].length;
        const to = from + m[0].length - m[1].length;
        items.push({
          from,
          to,
          deco: Decoration.replace({
            widget: new CheckboxWidget(m[2] === "x" || m[2] === "X", line.from),
          }),
        });
      } else if (line.text.trim()) {
        items.push({
          from: line.from,
          to: line.from,
          deco: Decoration.widget({ widget: new CheckboxWidget(false, line.from), side: -1 }),
        });
      }
    }
    return Decoration.set(items.map((i) => i.deco.range(i.from, i.to)), true);
  }

  const checkboxPlugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(v: EditorView) { this.decorations = buildDecos(v.state); }
      update(u: ViewUpdate) {
        if (u.docChanged) this.decorations = buildDecos(u.state);
      }
    },
    { decorations: (v) => v.decorations },
  );

  // Enter продолжает список — как в любом редакторе с чек-листами. Без этого
  // новая строка осталась бы без разметки, и отметить её было бы нечем.
  const enterKeymap = keymap.of([
    {
      key: "Enter",
      run(v) {
        const { state } = v;
        const line = state.doc.lineAt(state.selection.main.head);
        // Продолжаем список и после строки без префикса: она всё равно станет
        // подзадачей при сохранении, поэтому требовать разметку от следующей
        // строки было бы непоследовательно (и именно так первая набранная
        // руками подзадача оставалась без чекбокса).
        const body = line.text.replace(CHECK_RE, "").trim();
        if (!body) return false; // пустая строка — обычный перенос
        const insert = "\n[ ] ";
        const at = state.selection.main.head;
        v.dispatch({
          changes: { from: at, to: at, insert },
          selection: { anchor: at + insert.length },
        });
        return true;
      },
    },
  ]);

  const theme = EditorView.theme({
    "&": { fontSize: "13px" },
    "&.cm-focused": { outline: "none" },
    ".cm-content": { padding: "4px 6px", fontFamily: "inherit", caretColor: "var(--text)" },
    ".cm-line": { padding: "0" },
    ".cm-scroller": { fontFamily: "inherit", lineHeight: "20px" },
    ".cm-sub-checkbox": { marginRight: "6px", cursor: "pointer", verticalAlign: "middle" },
    ".cm-placeholder": { color: "var(--text-secondary)" },
  });

  onMount(() => {
    if (!hostEl) return;
    view = new EditorView({
      parent: hostEl,
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          drawSelection(),
          checkboxPlugin,
          enterKeymap,
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          cmPlaceholder(placeholder),
          theme,
          EditorView.updateListener.of((u) => {
            if (!u.docChanged) return;
            value = u.state.doc.toString();
            onchange?.(value);
          }),
        ],
      }),
    });
  });

  onDestroy(() => view?.destroy());

  // Внешняя подмена значения (загрузка слота, применение шаблона). Сравнение
  // с текущим документом обязательно: без него собственная правка вернулась бы
  // сюда через bind и переставила каретку в конец на каждой букве.
  $effect(() => {
    const next = value;
    if (view && next !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
    }
  });
</script>

<div class="checklist-editor" bind:this={hostEl}></div>

<style>
  .checklist-editor {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-input, var(--bg-card));
    max-height: 30vh;
    overflow-y: auto;
  }
  .checklist-editor :global(.cm-editor) {
    background: transparent;
  }
</style>
