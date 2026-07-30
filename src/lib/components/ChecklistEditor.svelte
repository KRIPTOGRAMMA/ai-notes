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
  import {
    CHECK_RE, toggleLine, lineIndexAt, removeLineAt, emptyAfterBackspace,
    dropEmptyLines, repairChecklistMarkup,
  } from "../checklistText";

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
  function newSubtaskLine(v: EditorView): boolean {
    const { state } = v;
    const at = state.selection.main.head;
    // Разметку получает КАЖДАЯ новая строка, в том числе после пустой
    // (v0.9.50). Раньше здесь стоял выход при пустой текущей строке, и Enter
    // на ней проваливался в defaultKeymap — появлялась строка без чекбокса,
    // которой в чек-листе быть не может: каждая строка тут подзадача.
    const insert = "\n[ ] ";
    v.dispatch({
      changes: { from: at, to: at, insert },
      selection: { anchor: at + insert.length },
    });
    return true;
  }

  const enterKeymap = keymap.of([
    { key: "Enter", run: newSubtaskLine },
    // Shift+Enter привязан явно, хотя defaultKeymap и так шлёт его в Enter
    // (проверено в браузере: без этой строки поведение то же). Объявлено,
    // чтобы оно не держалось на внутренней детали чужого keymap: в чек-листе
    // каждая строка — подзадача, разрывов абзаца здесь не бывает, поэтому обе
    // комбинации обязаны делать одно и то же.
    { key: "Shift-Enter", run: newSubtaskLine },
    // Ctrl/Cmd+Enter принадлежит окну, а не редактору (v0.9.51): в быстром
    // слоте это «сохранить», в модалке задачи — тоже. Без явной привязки
    // комбинация проваливалась в defaultKeymap и вставляла пустую строку без
    // разметки — список ломался, а сохранение не срабатывало.
    //
    // Возвращаем true (комбинация обработана — CodeMirror ничего не делает),
    // но событие не гасим: keydown всплывает до <svelte:window> снаружи,
    // который и вызывает submit.
    { key: "Mod-Enter", run: () => true },
  ]);

  // Backspace, стирающий последнюю букву подзадачи, убирает её целиком
  // (v0.9.48).
  //
  // Разметка `[ ] ` спрятана виджетом, поэтому для пользователя строка — это
  // её текст. Подзадачи стирают с конца, а не ставят каретку в начало: когда
  // исчезает последняя буква, подзадача должна исчезнуть вместе с ней. Иначе
  // на экране остаётся пустая строка с чекбоксом (в данных её уже нет —
  // parseChecklist пустые выбрасывает), и требуется ещё одно нажатие по
  // невидимым скобкам.
  //
  // Условие — «после этого удаления текста не останется», а не «каретка в
  // начале строки»: второе описывает способ, которым до строки добираются,
  // первое — то, что пользователь считает удалением подзадачи.
  //
  // Срабатывает только при схлопнутом выделении: выделенный кусок текста
  // Backspace должен удалять как обычно, а не сносить строку.
  const backspaceKeymap = keymap.of([
    {
      key: "Backspace",
      run(v) {
        const { state } = v;
        const sel = state.selection.main;
        if (!sel.empty) return false;
        const line = state.doc.lineAt(sel.head);
        // Текст в строке остаётся — обычное удаление символа.
        if (!emptyAfterBackspace(line.text, sel.head - line.from)) return false;

        // Единственная пустая строка: удалять нечего, иначе Backspace на
        // пустом поле «съел» бы его целиком без всякого видимого повода.
        if (state.doc.lines === 1) return false;

        const doc = state.doc.toString();
        const next = removeLineAt(doc, sel.head);
        // Каретка — в конец предыдущей строки, как при обычном Backspace на
        // стыке строк (line.from - 1 — позиция её последнего символа после
        // удаления перевода). Для первой строки ставим в начало документа.
        const anchor = line.number > 1 ? line.from - 1 : 0;
        v.dispatch({
          changes: { from: 0, to: doc.length, insert: next },
          selection: { anchor: Math.min(anchor, next.length) },
        });
        return true;
      },
    },
  ]);

  // Разметку нельзя испортить никаким удалением (v0.9.50).
  //
  // Свой Backspace закрывал только одну клавишу. Ctrl+Backspace (удалить
  // слово) шёл мимо него и выедал скобки изнутри: в строке оставался видимый
  // огрызок «[ » — та самая разметка, которую пользователю показывать нельзя.
  // То же самое дают Delete, Ctrl+Delete и вставка поверх выделения.
  //
  // Поэтому чиним не клавиши по списку, а результат: если после изменения
  // строка потеряла целостность (текст есть, а разметка битая), приводим её к
  // корректному виду. Перечислять комбинации бессмысленно — их больше, чем
  // можно предугадать, и каждая новая версия CodeMirror может добавить свои.
  const repairMarkup = EditorState.transactionFilter.of((tr) => {
    if (!tr.docChanged) return tr;
    const text = tr.newDoc.toString();
    const fixed = repairChecklistMarkup(text);
    if (fixed === text) return tr;
    // Каретку держим на месте: чинится то, что левее её, поэтому смещение
    // считаем по разнице длин до позиции каретки.
    const head = tr.newSelection.main.head;
    const delta = fixed.length - text.length;
    return [
      { changes: { from: 0, to: tr.startState.doc.length, insert: fixed },
        selection: { anchor: Math.max(0, Math.min(head + delta, fixed.length)) },
        scrollIntoView: true },
    ];
  });

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
          repairMarkup,
          // Свои обработчики — до defaultKeymap: тот перехватил бы Backspace
          // и удалил один невидимый символ разметки вместо строки.
          enterKeymap,
          backspaceKeymap,
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          cmPlaceholder(placeholder),
          theme,
          // Уход фокуса — момент, когда правка закончена: пустые строки
          // подчищаются здесь, а не по ходу набора (v0.9.49). Иначе строка
          // исчезала бы под кареткой ровно тогда, когда пользователь нажал
          // Enter и собрался печатать название.
          //
          // Замена документа рассылает обычное изменение, поэтому
          // updateListener ниже сам отдаст очищенный текст наружу.
          EditorView.domEventHandlers({
            blur(_e, v) {
              const doc = v.state.doc.toString();
              const cleaned = dropEmptyLines(doc);
              if (cleaned === doc) return false;
              v.dispatch({
                changes: { from: 0, to: doc.length, insert: cleaned },
              });
              return false;
            },
          }),
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
