<script lang="ts">
  // Живой markdown-редактор (Obsidian-style live preview) на CodeMirror 6.
  // Один режим: заголовки/жирный/курсив/код/списки/чекбоксы/[[ссылки]] рендерятся
  // инлайн прямо в тексте; синтаксис-маркеры (##, **, [[ ]]) видны только на
  // строке, где сейчас курсор — иначе редактирование было бы вслепую.
  //
  // Примечание: @codemirror/lang-markdown сам продолжает маркер списка ("- ")
  // при Enter внутри пункта списка — стандартное поведение таких редакторов.
  // Программная вставка многострочного текста с "\n" через keyboard.type()
  // (а не paste/insertText) в e2e триггерит ту же логику и дублирует маркер —
  // учтено в e2e-хелпере fillNoteEditor (использует insertText).
  import { onMount, onDestroy } from "svelte";
  import { EditorState, EditorSelection, StateField, type Extension } from "@codemirror/state";
  import {
    EditorView, Decoration, type DecorationSet, WidgetType, keymap, ViewPlugin, type ViewUpdate,
    drawSelection, dropCursor, placeholder as cmPlaceholder,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { markdown } from "@codemirror/lang-markdown";
  import { syntaxTree } from "@codemirror/language";
  import {
    autocompletion, completionKeymap,
    type CompletionContext, type CompletionResult,
  } from "@codemirror/autocomplete";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api } from "../api/tauri";
  import { IMAGE_RE, imageMarkdown, extImageExt, parseTableAt, serializeTable, emptyTable, type ParsedTable, type TableAlign } from "../markdown";

  let {
    value = $bindable(""),
    placeholder: placeholderText = "",
    knownTitles = [],
    resolveExists = () => false,
    onWikiLinkClick,
    onSubmitShortcut,
  }: {
    value: string;
    placeholder?: string;
    knownTitles?: string[];
    resolveExists?: (title: string) => boolean;
    onWikiLinkClick?: (title: string) => void;
    onSubmitShortcut?: () => void;
  } = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;

  // knownTitles/resolveExists меняются реактивно (список заметок), но не
  // должны пересоздавать редактор — читаем их через мутable-обёртку, которую
  // decoration-плагин видит при каждом рефреше. Заполняется в $effect ниже
  // (а не тут), чтобы не захватывать только начальное значение пропа.
  const linkCtx: {
    knownTitles: string[];
    resolveExists: (title: string) => boolean;
    onWikiLinkClick?: (title: string) => void;
  } = { knownTitles: [], resolveExists: () => false };
  $effect(() => {
    linkCtx.knownTitles = knownTitles;
    linkCtx.resolveExists = resolveExists;
    linkCtx.onWikiLinkClick = onWikiLinkClick;
    forceRebuild = true;
    // Пустая транзакция — единственный способ дёрнуть ViewPlugin.update()
    // без реального изменения документа/выделения.
    view?.dispatch({});
  });

  class CheckboxWidget extends WidgetType {
    checked: boolean;
    pos: number;
    constructor(checked: boolean, pos: number) {
      super();
      this.checked = checked;
      this.pos = pos;
    }
    eq(other: CheckboxWidget) { return other.checked === this.checked && other.pos === this.pos; }
    toDOM() {
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = this.checked;
      box.className = "cm-task-checkbox";
      box.onmousedown = (e) => e.preventDefault(); // не отдавать фокус чекбоксу
      box.onclick = () => {
        if (!view) return;
        const line = view.state.doc.lineAt(this.pos);
        const text = line.text;
        const next = text.replace(/\[( |x|X)\]/, (_m, mark: string) => `[${mark === " " ? "x" : " "}]`);
        view.dispatch({ changes: { from: line.from, to: line.to, insert: next } });
      };
      return box;
    }
    ignoreEvent() { return false; }
  }

  class WikiLinkWidget extends WidgetType {
    target: string;
    label: string;
    constructor(target: string, label: string) {
      super();
      this.target = target;
      this.label = label;
    }
    eq(other: WikiLinkWidget) { return other.target === this.target && other.label === this.label; }
    toDOM() {
      const a = document.createElement("a");
      a.href = "#";
      a.className = "cm-wikilink";
      a.textContent = this.label;
      const exists = linkCtx.resolveExists(this.target);
      if (!exists) a.classList.add("missing");
      a.title = exists ? this.target : `Создать «${this.target}»`;
      a.onmousedown = (e) => e.preventDefault();
      a.onclick = (e) => {
        e.preventDefault();
        linkCtx.onWikiLinkClick?.(this.target);
      };
      return a;
    }
    ignoreEvent() { return false; }
  }

  // Абсолютный путь к папке images резолвится один раз при монтировании
  // (get_images_dir) — convertFileSrc() требует абсолютный путь, а markdown
  // хранит только имя файла (см. paste-обработчик ниже).
  let imagesDir: string | null = null;
  api.getImagesDir().then(d => { imagesDir = d; forceRebuild = true; view?.dispatch({}); }).catch(() => {});

  // Картинки, у которых сейчас кликом раскрыта markdown-ссылка рядом с
  // рендером (по умолчанию видна только картинка). Ключ — "from:to" диапазона
  // ![](...) в документе; переживает только пока сам диапазон не меняется
  // (правка текста выше по документу сдвинет позиции — раскрытые вернутся
  // к дефолту, что нормально для редких кликов).
  const revealedImages = new Set<string>();

  class ImageWidget extends WidgetType {
    filename: string;
    dir: string | null;
    key: string;
    constructor(filename: string, from: number, to: number) {
      super();
      this.filename = filename;
      this.dir = imagesDir;
      this.key = `${from}:${to}`;
    }
    // imagesDir резолвится асинхронно после монтирования (см. ниже) — включаем
    // снимок dir в eq(), иначе CodeMirror переиспользует DOM-узел, созданный ДО
    // того, как путь стал известен, и src так и останется пустым до следующей правки.
    eq(other: ImageWidget) { return other.filename === this.filename && other.dir === this.dir && other.key === this.key; }
    toDOM() {
      const img = document.createElement("img");
      img.className = "cm-note-image";
      if (this.dir) {
        img.src = convertFileSrc(`${this.dir}/${this.filename}`);
      }
      img.alt = this.filename;
      img.onerror = () => img.classList.add("broken");
      img.title = "Клик — показать/скрыть ссылку";
      img.onmousedown = (e) => e.preventDefault();
      img.onclick = () => {
        if (revealedImages.has(this.key)) revealedImages.delete(this.key);
        else revealedImages.add(this.key);
        forceRebuild = true;
        view?.dispatch({});
      };
      return img;
    }
    ignoreEvent() { return false; }
  }

  // Таблица (v0.9.06): единственный виджет, представляющий многострочный
  // блок как одну DOM-структуру — click-to-edit оверлей поверх реального
  // <table>, а не просто подсветка синтаксиса (не влезает в mark-decoration
  // паттерн заголовков/жирного: нужна настоящая 2D-раскладка ячеек).
  // Правки в ячейках накапливаются в this.table (мутируется на месте) и
  // сериализуются обратно в markdown одним view.dispatch на blur/Tab/Enter —
  // не на каждый keystroke, иначе каждая буква пересобирала бы весь виджет
  // и сбрасывала фокус/каретку в contenteditable-ячейке.
  // Ячейка, которую нужно сфокусировать после ближайшей пересборки виджета
  // (Tab/Enter в ячейке коммитят правку → CM6 синхронно пересобирает DOM
  // таблицы → старые ссылки на DOM-узлы протухают вместе с замкнутыми
  // cellsGrid()/headRow/tbody). toDOM() читает и сбрасывает этот флаг сразу
  // после построения новой DOM-структуры — межвиджетный, а не per-instance,
  // потому что "следующий" виджет — это буквально другой JS-объект.
  let pendingTableFocus: { rowIndex: number; colIndex: number } | null = null;

  class TableWidget extends WidgetType {
    table: ParsedTable;
    constructor(table: ParsedTable) {
      super();
      this.table = table;
    }
    // eq() сравнивает только содержимое таблицы, не диапазон — CodeMirror
    // переиспользует старый DOM-узел виджета (и, что важно, сам JS-инстанс)
    // при любой правке документа, если новый TableWidget оказался "равен"
    // старому. Диапазон [from, to) поэтому НЕ хранится в полях инстанса
    // (там он бы протух после первой же правки, ведущей к пересозданию
    // виджета с новыми позициями, пока переиспользуется старый JS-объект) —
    // вместо этого commit() всегда находит текущую позицию блока заново
    // через view.posAtDOM(wrap), на момент самого коммита.
    eq(other: TableWidget) {
      return JSON.stringify(other.table) === JSON.stringify(this.table);
    }
    commit(wrap: HTMLElement, next: ParsedTable) {
      if (!view || !wrap.isConnected) return;
      // Правки в разных ячейках (клик по кнопке "+ строка"/переход по Tab)
      // могут инициировать почти одновременный blur старой ячейки и клик
      // новой команды — оба пытаются закоммитить одну и ту же (уже
      // устаревшую к моменту второго вызова) DOM-структуру виджета.
      // wrap.isConnected выше отсеивает большинство случаев, но CM6 может
      // отсоединить узел синхронно, уже во время выполнения posAtDOM/dispatch
      // ниже (реентрантно, изнутри своего же цикла DOM-обновления) — поэтому
      // ловим исключение вместо попытки предугадать любую гонку заранее:
      // устаревший коммит — по определению no-op, а не то, что стоит чинить
      // жёстче ценой более хрупкой логики синхронизации.
      try {
        const from = view.posAtDOM(wrap);
        const line = view.state.doc.lineAt(from);
        const parsed = parseTableAt(view.state.doc.toString(), line.number);
        if (!parsed) return; // документ уже не начинается с таблицы в этой позиции — не коммитим вслепую
        const to = view.state.doc.line(parsed.endLine).to;
        const md = serializeTable(next);
        view.dispatch({ changes: { from: line.from, to, insert: md } });
      } catch {
        // Гонка на пересборке DOM виджета — коммитить уже нечего, безопасно игнорировать.
      }
    }
    toDOM() {
      const wrap = document.createElement("div");
      wrap.className = "cm-table-wrap";
      const table = document.createElement("table");
      table.className = "cm-table";
      wrap.appendChild(table);

      const alignStyle = (a: TableAlign) => a ? `text-align:${a};` : "";

      const thead = document.createElement("thead");
      const headRow = document.createElement("tr");
      this.table.header.forEach((text, c) => {
        const th = document.createElement("th");
        th.contentEditable = "true";
        th.textContent = text;
        th.style.cssText = alignStyle(this.table.align[c]);
        wireCell(th, 0, c);
        headRow.appendChild(th);
      });
      thead.appendChild(headRow);
      table.appendChild(thead);

      const tbody = document.createElement("tbody");
      this.table.rows.forEach((row, r) => {
        const tr = document.createElement("tr");
        row.forEach((text, c) => {
          const td = document.createElement("td");
          td.contentEditable = "true";
          td.textContent = text;
          td.style.cssText = alignStyle(this.table.align[c]);
          wireCell(td, r + 1, c);
          tr.appendChild(td);
        });
        tbody.appendChild(tr);
      });
      table.appendChild(tbody);

      const toolbar = document.createElement("div");
      toolbar.className = "cm-table-toolbar";
      const addRowBtn = document.createElement("button");
      addRowBtn.textContent = "+ строка";
      addRowBtn.onmousedown = (e) => e.preventDefault();
      addRowBtn.onclick = () => {
        const next: ParsedTable = { ...this.table, rows: [...this.table.rows, this.table.header.map(() => "")] };
        this.commit(wrap, next);
      };
      const addColBtn = document.createElement("button");
      addColBtn.textContent = "+ столбец";
      addColBtn.onmousedown = (e) => e.preventDefault();
      addColBtn.onclick = () => {
        const next: ParsedTable = {
          header: [...this.table.header, `Колонка ${this.table.header.length + 1}`],
          align: [...this.table.align, null],
          rows: this.table.rows.map(r => [...r, ""]),
        };
        this.commit(wrap, next);
      };
      toolbar.appendChild(addRowBtn);
      toolbar.appendChild(addColBtn);
      wrap.appendChild(toolbar);

      // rowIndex 0 = заголовок, 1..N = тело (table.rows[rowIndex-1])
      const self = this;
      // Tab/Enter коммитят явно и сразу после сами переводят фокус — из-за
      // чего у старой ячейки тоже срабатывает blur (фокус ушёл с неё) и
      // повторно зовёт commitFromDom() на уже отсоединённом wrap этого же
      // (старого) DOM-дерева. wrap.isConnected в TableWidget.commit() уже
      // отсеивает часть случаев, но повторный вызов может попасть ровно в
      // момент, когда CM6 ещё синхронно перестраивает DOM после первого
      // коммита (реентрантный dispatch) — проще и надёжнее не пытаться
      // коммитить дважды с одного и того же построения виджета вообще.
      let committedOnce = false;
      function cellsGrid(): HTMLElement[][] {
        const headCells = Array.from(headRow.children) as HTMLElement[];
        const bodyRows = Array.from(tbody.children).map(tr => Array.from(tr.children) as HTMLElement[]);
        return [headCells, ...bodyRows];
      }
      function readCellText(el: HTMLElement): string {
        return el.textContent ?? "";
      }
      function commitFromDom() {
        if (committedOnce) return;
        committedOnce = true;
        const grid = cellsGrid();
        const header = grid[0].map(readCellText);
        const rows = grid.slice(1).map(r => r.map(readCellText));
        self.commit(wrap, { header, align: self.table.align, rows });
      }
      function focusCell(rowIndex: number, colIndex: number) {
        const grid = cellsGrid();
        const row = grid[Math.max(0, Math.min(grid.length - 1, rowIndex))];
        if (!row) return;
        const cell = row[Math.max(0, Math.min(row.length - 1, colIndex))];
        cell?.focus();
        // Каретку — в конец текста ячейки, иначе фокус ставится перед текстом.
        if (cell) {
          const range = document.createRange();
          range.selectNodeContents(cell);
          range.collapse(false);
          const sel = window.getSelection();
          sel?.removeAllRanges();
          sel?.addRange(range);
        }
      }
      function wireCell(el: HTMLElement, rowIndex: number, colIndex: number) {
        el.onblur = () => commitFromDom();
        el.onkeydown = (e) => {
          // Останавливаем всплытие к CM6-кеймапу (Mod-b и т.п. не должны
          // применяться внутри ячейки таблицы, это её содержимое, а не
          // документ редактора).
          e.stopPropagation();
          // Ctrl/Cmd+A: contenteditable="false" на обёртке виджета НЕ создаёт
          // отдельный edit-host для Selection API в Chromium — нативный
          // select-all внутри вложенного contenteditable="true" всё равно
          // выделяет весь contentDOM CM6 целиком (проверено вручную: после
          // Ctrl+A в ячейке window.getSelection() отдавал текст всего
          // документа). Поэтому выделение "всё в этой ячейке" делаем сами
          // через Range/Selection API, а не полагаемся на браузер.
          if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
            e.preventDefault();
            const range = document.createRange();
            range.selectNodeContents(el);
            const sel = window.getSelection();
            sel?.removeAllRanges();
            sel?.addRange(range);
            return;
          }
          if (e.key === "Tab") {
            e.preventDefault();
            const grid = cellsGrid();
            const rowLen = grid[rowIndex]?.length ?? 0;
            let target: { rowIndex: number; colIndex: number };
            if (e.shiftKey) {
              target = colIndex > 0
                ? { rowIndex, colIndex: colIndex - 1 }
                : { rowIndex: rowIndex - 1, colIndex: (grid[rowIndex - 1]?.length ?? 1) - 1 };
            } else {
              target = colIndex < rowLen - 1
                ? { rowIndex, colIndex: colIndex + 1 }
                : { rowIndex: rowIndex + 1, colIndex: 0 };
            }
            // commitFromDom() пересобирает DOM таблицы синхронно внутри
            // view.dispatch — сохраняем цель фокуса заранее, а не зовём
            // focusCell() сразу после: к этому моменту headRow/tbody уже
            // могут указывать на удалённые узлы старого виджета.
            pendingTableFocus = target;
            commitFromDom();
          } else if (e.key === "Enter") {
            e.preventDefault();
            pendingTableFocus = { rowIndex: rowIndex + 1, colIndex };
            commitFromDom();
          } else if (e.key === "Escape") {
            (el as HTMLElement).blur();
          }
        };
      }

      // Если предыдущая ячейка (в старом, уже удалённом виджете) запросила
      // фокус после коммита — выполняем это здесь, в свежепостроенном DOM.
      if (pendingTableFocus) {
        const target = pendingTableFocus;
        pendingTableFocus = null;
        queueMicrotask(() => focusCell(target.rowIndex, target.colIndex));
      }

      return wrap;
    }
    // ignoreEvent(event) === true означает "CodeMirror, не трогай это событие
    // вообще" (см. eventBelongsToEditor в @codemirror/view — при true CM6 не
    // запускает на нём ни один свой обработчик/кеймап) — нужно для кликов по
    // ячейкам/кнопкам +строка/+столбец, иначе CM6 перехватывает mousedown и
    // не даёт реально кликнуть внутрь виджета.
    ignoreEvent(event: Event) {
      return event.type === "mousedown" || event.type === "click" || event.type === "keydown" || event.type === "blur";
    }
  }

  // Собирает Lezer-диапазоны кода (FencedCode, InlineCode), чтобы
  // не применять инлайн-стили и вики-ссылки внутри них.
  function codeRanges(state: EditorState): Set<number> {
    const set = new Set<number>();
    let depth = 0;
    syntaxTree(state).iterate({
      from: 0,
      to: state.doc.length,
      enter: (node) => {
        if (node.name === "FencedCode" || node.name === "InlineCode") {
          depth++;
          for (let p = node.from; p < node.to; p++) set.add(p);
          return false;
        }
        return undefined;
      },
    });
    return set;
  }

  function inCode(pos: number, set: Set<number>): boolean {
    return set.has(pos);
  }

  // Таблицы — блочные decoration'ы (block: true), а CodeMirror запрещает
  // блочные decoration'ы из ViewPlugin-источника (динамического facet) —
  // "Block decorations may not be specified via plugins". Поэтому таблицы
  // строятся отдельным StateField (статический источник), не вместе с
  // остальным live-preview в livePreviewPlugin. Раз это блочный виджет,
  // не однострочная mark-decoration, скрывать его "только пока в фокусе"
  // не нужно — так же, как ImageWidget не проверяет hasFocus. Курсор
  // внутри диапазона строк таблицы (по selection, не завязано на hasFocus)
  // показывает сырой markdown для редактирования textual-diff/копирования.
  function buildTableDecorations(state: EditorState): DecorationSet {
    const cursorLine = state.doc.lineAt(state.selection.main.head).number;
    const codePositions = codeRanges(state);
    const items: { from: number; to: number; deco: Decoration }[] = [];
    const docText = state.doc.toString();

    for (let i = 1; i <= state.doc.lines; i++) {
      const line = state.doc.line(i);
      if (inCode(line.from, codePositions) || !line.text.includes("|")) continue;
      const parsed = parseTableAt(docText, i);
      if (!parsed) continue;
      const lastLineNum = parsed.endLine;
      const cursorInBlock = cursorLine >= i && cursorLine <= lastLineNum;
      if (!cursorInBlock) {
        const blockFrom = line.from;
        const blockTo = state.doc.line(lastLineNum).to;
        items.push({
          from: blockFrom, to: blockTo,
          deco: Decoration.replace({ widget: new TableWidget(parsed.table), block: true }),
        });
      }
      i = lastLineNum; // следующая итерация цикла (i++) продолжит сразу за таблицей
    }

    return Decoration.set(items.map(it => it.deco.range(it.from, it.to)), true);
  }

  const tableField = StateField.define<DecorationSet>({
    create(state) { return buildTableDecorations(state); },
    update(deco, tr) {
      return tr.docChanged || tr.selection ? buildTableDecorations(tr.state) : deco;
    },
    provide: f => EditorView.decorations.from(f),
  });

  // Строит decoration-набор для всего документа: строка с курсором показывает
  // сырой markdown (но только пока редактор реально в фокусе — иначе после
  // программной подмены value/пересинхронизации курсор на строке 1 навсегда
  // прятал бы виджеты в однострочных заметках), остальные — отрендеренный вид.
  // Таблицы сюда не входят — см. tableField выше.
  function buildDecorations(state: EditorState, hasFocus: boolean): DecorationSet {
    const cursorLine = hasFocus ? state.doc.lineAt(state.selection.main.head).number : -1;
    const codePositions = codeRanges(state);
    const items: { from: number; to: number; deco: Decoration }[] = [];

    for (let i = 1; i <= state.doc.lines; i++) {
      const line = state.doc.line(i);
      const raw = i === cursorLine;
      const text = line.text;

      // Заголовки: строку целиком метим классом размера, маркер '#' скрываем
      const hLevel = text.startsWith("#") ? /^#{1,6}/.exec(text)?.[0].length ?? 0 : 0;
      if (hLevel > 0) {
        items.push({
          from: line.from, to: line.from,
          deco: Decoration.line({ class: `cm-h cm-h${hLevel}` }),
        });
        if (!raw) {
          items.push({
            from: line.from, to: line.from + hLevel + 1,
            deco: Decoration.replace({}),
          });
        }
      }

      // Чекбоксы: "- [ ] " / "- [x] " → виджет
      const cbMatch = /^(\s*[-*+]\s+)\[( |x|X)\]/.exec(text);
      if (cbMatch) {
        const markStart = line.from + cbMatch[1].length;
        const markEnd = markStart + 3;
        const checked = cbMatch[2].toLowerCase() === "x";
        items.push({
          from: markStart, to: markEnd,
          deco: Decoration.replace({ widget: new CheckboxWidget(checked, line.from) }),
        });
      }

      // Картинки ![alt](filename) — НЕ внутри кода. По умолчанию видна только
      // отрендеренная картинка (markdown-ссылка скрыта); клик по картинке
      // раскрывает ссылку рядом с ней (revealedImages), повторный клик —
      // прячет обратно. Картинка — Decoration.widget (side: 1) сразу после
      // текста, а не replace: сама ссылка отдельно скрывается/показывается
      // через Decoration.replace по тому же диапазону.
      for (const m of text.matchAll(IMAGE_RE)) {
        const from = line.from + m.index!;
        const to = from + m[0].length;
        if (inCode(from, codePositions)) continue;
        const filename = m[2].trim();
        if (!filename) continue;
        const key = `${from}:${to}`;
        if (!revealedImages.has(key)) {
          items.push({ from, to, deco: Decoration.replace({}) });
        }
        items.push({
          from: to, to,
          deco: Decoration.widget({ widget: new ImageWidget(filename, from, to), side: 1 }),
        });
      }

      if (!raw) {
        const lineStart = line.from;

        // Жирный **text** — только не внутри кода
        if (!inCode(lineStart, codePositions)) {
          for (const m of text.matchAll(/\*\*([^*\n]+)\*\*/g)) {
            const from = lineStart + m.index!;
            const to = from + m[0].length;
            items.push({ from, to: from + 2, deco: Decoration.replace({}) });
            items.push({ from: from + 2, to: to - 2, deco: Decoration.mark({ class: "cm-strong" }) });
            items.push({ from: to - 2, to, deco: Decoration.replace({}) });
          }
          // Курсив *text*/_text_ — только не внутри кода
          for (const m of text.matchAll(/(?<!\*)\*([^*\n]+)\*(?!\*)|(?<!_)_([^_\n]+)_(?!_)/g)) {
            const from = lineStart + m.index!;
            const to = from + m[0].length;
            items.push({ from, to: from + 1, deco: Decoration.replace({}) });
            items.push({ from: from + 1, to: to - 1, deco: Decoration.mark({ class: "cm-em" }) });
            items.push({ from: to - 1, to, deco: Decoration.replace({}) });
          }
          // Инлайн-код `code`
          for (const m of text.matchAll(/`([^`\n]+)`/g)) {
            const from = lineStart + m.index!;
            const to = from + m[0].length;
            items.push({ from, to: from + 1, deco: Decoration.replace({}) });
            items.push({ from: from + 1, to: to - 1, deco: Decoration.mark({ class: "cm-code" }) });
            items.push({ from: to - 1, to, deco: Decoration.replace({}) });
          }
          // Вики-ссылки [[target]] / [[target|label]] — НЕ внутри кода
          for (const m of text.matchAll(/\[\[([^\[\]|]+)(?:\|([^\[\]]+))?\]\]/g)) {
            const from = lineStart + m.index!;
            const to = from + m[0].length;
            if (inCode(from, codePositions)) continue;
            const target = m[1].trim();
            const label = (m[2] ?? m[1]).trim();
            if (!target) continue;
            items.push({
              from, to,
              deco: Decoration.replace({ widget: new WikiLinkWidget(target, label) }),
            });
          }
        }
      }
    }

    // FencedCode → CodeText: моноширинный фон через Lezer-дерево
    syntaxTree(state).iterate({
      from: 0,
      to: state.doc.length,
      enter: (node) => {
        if (node.name === "FencedCode") {
          let child = node.node.firstChild;
          while (child) {
            if (child.name === "CodeText") {
              items.push({
                from: child.from, to: child.to,
                deco: Decoration.mark({ class: "cm-code" }),
              });
            }
            child = child.nextSibling;
          }
          return false;
        }
        return undefined;
      },
    });

    return Decoration.set(
      items.map(it => it.deco.range(it.from, it.to)),
      true,
    );
  }

  // ViewPlugin, а не StateField: raw/rendered решение зависит от view.hasFocus,
  // которое StateField в принципе не видит (у него нет доступа к EditorView).
  const livePreviewPlugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(v: EditorView) {
        this.decorations = buildDecorations(v.state, v.hasFocus);
      }
      update(u: ViewUpdate) {
        if (u.docChanged || u.selectionSet || u.focusChanged || forceRebuild) {
          this.decorations = buildDecorations(u.state, u.view.hasFocus);
          forceRebuild = false;
        }
      }
    },
    { decorations: v => v.decorations },
  );
  // Флаг «внешние knownTitles/resolveExists изменились» — ViewPlugin.update
  // не запускается сам по себе на реактивные пропы, только на события CM;
  // дёргаем через view.dispatch (пустой transaction всё равно вызывает update).
  let forceRebuild = false;

  function wikiLinkCompletion(context: CompletionContext): CompletionResult | null {
    const word = context.matchBefore(/\[\[[^\[\]]*/);
    if (!word) return null;
    const query = word.text.slice(2).toLowerCase();
    const options = linkCtx.knownTitles
      .filter(t => t.toLowerCase().includes(query))
      .slice(0, 8)
      .map(t => ({ label: t, apply: `${t}]]` }));
    if (options.length === 0) return null;
    return { from: word.from + 2, options, filter: false };
  }

  const theme = EditorView.theme({
    "&": { height: "100%", fontSize: "13px" },
    ".cm-scroller": { fontFamily: "inherit", lineHeight: "1.6", overflow: "auto" },
    ".cm-content": { padding: "12px 14px" },
    "&.cm-focused": { outline: "none" },
    ".cm-h": { fontWeight: "600" },
    ".cm-h1": { fontSize: "1.5em" },
    ".cm-h2": { fontSize: "1.3em" },
    ".cm-h3": { fontSize: "1.15em" },
    ".cm-h4, .cm-h5, .cm-h6": { fontSize: "1.05em" },
    ".cm-strong": { fontWeight: "700" },
    ".cm-em": { fontStyle: "italic" },
    ".cm-code": {
      fontFamily: "monospace",
      background: "var(--bg-secondary)",
      padding: "1px 4px",
      borderRadius: "4px",
      fontSize: "0.9em",
    },
    ".cm-wikilink": {
      textDecoration: "none",
      borderBottom: "1px solid color-mix(in srgb, var(--accent) 45%, transparent)",
      color: "var(--accent)",
      cursor: "pointer",
    },
    ".cm-wikilink.missing": {
      color: "var(--text-secondary)",
      borderBottomStyle: "dashed",
    },
    ".cm-task-checkbox": { marginRight: "4px", cursor: "pointer", verticalAlign: "middle" },
    ".cm-placeholder": { color: "var(--text-secondary)" },
    ".cm-note-image": {
      display: "block",
      maxWidth: "100%",
      marginTop: "4px",
      borderRadius: "6px",
    },
    ".cm-note-image.broken": {
      display: "inline-block",
      minWidth: "80px",
      minHeight: "40px",
      background: "var(--bg-secondary)",
      border: "1px dashed var(--border)",
    },
    ".cm-table-wrap": {
      margin: "6px 0",
    },
    ".cm-table": {
      borderCollapse: "collapse",
      width: "auto",
      maxWidth: "100%",
      fontSize: "0.95em",
    },
    ".cm-table th, .cm-table td": {
      border: "1px solid var(--border)",
      padding: "4px 8px",
      minWidth: "60px",
      outline: "none",
    },
    ".cm-table th": {
      background: "var(--bg-secondary)",
      fontWeight: "600",
    },
    ".cm-table td:focus, .cm-table th:focus": {
      boxShadow: "inset 0 0 0 1.5px var(--accent)",
    },
    ".cm-table-toolbar": {
      display: "flex",
      gap: "6px",
      marginTop: "4px",
    },
    ".cm-table-toolbar button": {
      fontSize: "11px",
      padding: "2px 8px",
      border: "1px solid var(--border)",
      borderRadius: "4px",
      background: "var(--bg-secondary)",
      color: "var(--text-secondary)",
      cursor: "pointer",
    },
    ".cm-table-toolbar button:hover": {
      color: "var(--text-primary)",
      borderColor: "var(--accent)",
    },
  });

  // Вставка картинки из буфера: перехватываем paste, если среди файлов буфера
  // есть image/* — сохраняем через save_note_image и вставляем ![](имя) на
  // месте курсора вместо стандартной вставки текста/пустоты.
  function fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
  }

  async function handleImagePaste(ev: ClipboardEvent, v: EditorView): Promise<boolean> {
    const items = ev.clipboardData?.items;
    // items.length === 0 — DOM ничего не увидел (WebKitGTK на Linux не прокидывает
    // изображения через ClipboardEvent вообще, даже когда в буфере реально image/png:
    // types и items приходят пустыми). Отличаем от «в буфере правда только текст» —
    // там items не пуст, просто type начинается не с "image/". Только для пустого
    // items имеет смысл идти в обходной путь через нативный буфер.
    if (!items || items.length === 0) {
      return pasteImageFromClipboard(ev, v);
    }
    const imageItem = Array.from(items).find(it => it.type.startsWith("image/"));
    if (!imageItem) return false;
    const file = imageItem.getAsFile();
    if (!file) return false;

    ev.preventDefault();
    try {
      const dataUrl = await fileToBase64(file);
      const ext = extImageExt(file.type || file.name || "png");
      const filename = await api.saveNoteImage(dataUrl, ext);
      const markdown = imageMarkdown(filename);
      const pos = v.state.selection.main.head;
      v.dispatch({
        changes: { from: pos, insert: markdown },
        selection: { anchor: pos + markdown.length },
      });
    } catch {
      // Сохранение не удалось (диск/права/мусор в буфере) — тихо ничего не вставляем.
    }
    return true;
  }

  // WebKitGTK на Linux (в т.ч. под Wayland/Hyprland) не прокидывает изображения
  // через ClipboardEvent.clipboardData — DOM paste-событие приходит пустым
  // (types: [], items: []), даже когда в буфере реально лежит image/png (проверено
  // вручную через `wl-paste --list-types`, показывает image/png; DOM-событие при
  // этом всё равно даёт items: []). Это ограничение самого WebKitGTK, а не кода
  // приложения. Обходим через нативный доступ к буферу — tauri-plugin-clipboard-manager
  // читает буфер через GTK API в обход DOM, но отдаёт сырой RGBA (Image.rgba() +
  // size()), не PNG — кодируем в PNG сами через canvas (toBlob), т.к. готового
  // PNG-энкодера в JS без доп. библиотек нет.
  async function rgbaToPngDataUrl(rgba: Uint8Array, width: number, height: number): Promise<string> {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 2d context unavailable");
    ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
    return new Promise((resolve, reject) => {
      canvas.toBlob(blob => {
        if (!blob) { reject(new Error("toBlob failed")); return; }
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      }, "image/png");
    });
  }

  // Возвращает true, только если реально вставили картинку (paste-обработчик
  // сверху решает, звать ли ev.preventDefault() — до этого момента буфер мог
  // содержать текст, который должен пройти обычной вставкой без перехвата).
  async function pasteImageFromClipboard(ev: ClipboardEvent, v: EditorView): Promise<boolean> {
    try {
      const { readImage } = await import("@tauri-apps/plugin-clipboard-manager");
      const image = await readImage();
      const [rgba, size] = await Promise.all([image.rgba(), image.size()]);
      ev.preventDefault();
      const dataUrl = await rgbaToPngDataUrl(rgba, size.width, size.height);
      const filename = await api.saveNoteImage(dataUrl, "png");
      const markdown = imageMarkdown(filename);
      const pos = v.state.selection.main.head;
      v.dispatch({
        changes: { from: pos, insert: markdown },
        selection: { anchor: pos + markdown.length },
      });
      return true;
    } catch {
      // В буфере не изображение (текст/пусто) или плагин недоступен — тихо
      // пропускаем, обычный Ctrl+V для текста проходит как есть (preventDefault
      // ещё не вызывался на этом пути, если readImage() упал раньше).
      return false;
    }
  }

  onMount(() => {
    if (!hostEl) return;
    const extensions: Extension[] = [
      history(),
      drawSelection(),
      dropCursor(),
      markdown(),
      livePreviewPlugin,
      tableField,
      autocompletion({ override: [wikiLinkCompletion] }),
      keymap.of([
        {
          key: "Mod-Enter",
          run: () => { onSubmitShortcut?.(); return true; },
        },
        { key: "Mod-b", run: () => { formatBold(); return true; } },
        { key: "Mod-i", run: () => { formatItalic(); return true; } },
        { key: "Mod-Shift-k", run: () => { formatWikiLink(); return true; } },
        ...historyKeymap,
        ...completionKeymap,
        ...defaultKeymap,
      ]),
      theme,
      EditorView.lineWrapping,
      cmPlaceholder(placeholderText),
      EditorView.domEventHandlers({
        paste: (event, v) => {
          void handleImagePaste(event, v);
          // Возврат из domEventHandlers не отменяет вставку сам по себе —
          // отмена делается через event.preventDefault() внутри handleImagePaste
          // (только когда среди буфера реально нашлась картинка).
          return false;
        },
      }),
      EditorView.updateListener.of(update => {
        if (update.docChanged) {
          value = update.state.doc.toString();
        }
      }),
    ];

    view = new EditorView({
      state: EditorState.create({ doc: value, extensions }),
      parent: hostEl,
    });
  });

  onDestroy(() => view?.destroy());

  // Внешние изменения value (смена заметки, вставка из composer/AI) —
  // синхронизируем документ, только если он реально разошёлся с state.
  $effect(() => {
    const v = value;
    if (view && view.state.doc.toString() !== v) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: v },
      });
    }
  });

  export function focus() {
    view?.focus();
  }

  // Форматирование из внешней панели инструментов (v0.9.05): оборачивает
  // выделение маркерами (жирный/курсив/код) или переключает префикс строки
  // (заголовок/чек-лист). Работает и без выделения — вставляет пустую пару
  // маркеров с курсором внутри (жирный/курсив/код) либо просто добавляет
  // префикс на текущей строке (заголовок/чек-лист/вики-ссылка).
  // Повторное нажатие на уже обёрнутом тексте снимает обёртку — иначе Ctrl+B
  // на **жирном** тексте продолжал бы плодить лишние ** снаружи (стандартное
  // поведение toggle-форматирования в любом текстовом редакторе).
  function wrapSelection(before: string, after: string) {
    if (!view) return;
    const { state } = view;
    const changes = state.changeByRange(range => {
      const selected = state.sliceDoc(range.from, range.to);
      const alreadyWrapped = !range.empty
        && selected.startsWith(before) && selected.endsWith(after)
        && selected.length >= before.length + after.length;
      if (alreadyWrapped) {
        const inner = selected.slice(before.length, selected.length - after.length);
        return {
          changes: [{ from: range.from, to: range.to, insert: inner }],
          range: EditorSelection.range(range.from, range.from + inner.length),
        };
      }
      const insertBefore = { from: range.from, insert: before };
      const insertAfter = { from: range.to, insert: after };
      return {
        changes: [insertBefore, insertAfter],
        range: range.empty
          ? EditorSelection.cursor(range.from + before.length)
          : EditorSelection.range(range.from + before.length, range.to + before.length),
      };
    });
    view.dispatch(state.update(changes, { scrollIntoView: true }));
    view.focus();
  }

  function toggleLinePrefix(prefix: string) {
    if (!view) return;
    const { state } = view;
    const changes = state.changeByRange(range => {
      const line = state.doc.lineAt(range.from);
      const has = line.text.startsWith(prefix);
      const change = has
        ? { from: line.from, to: line.from + prefix.length, insert: "" }
        : { from: line.from, insert: prefix };
      const delta = has ? -prefix.length : prefix.length;
      return {
        changes: [change],
        range: EditorSelection.range(range.from + delta, range.to + delta),
      };
    });
    view.dispatch(state.update(changes, { scrollIntoView: true }));
    view.focus();
  }

  export function formatBold() { wrapSelection("**", "**"); }
  export function formatItalic() { wrapSelection("*", "*"); }
  export function formatCode() { wrapSelection("`", "`"); }
  export function formatHeading() { toggleLinePrefix("## "); }
  export function formatChecklist() { toggleLinePrefix("- [ ] "); }
  export function formatWikiLink() { wrapSelection("[[", "]]"); }

  // Вставка таблицы (v0.9.06, добавлено в панель после первого прохода):
  // стартовая 2x2-таблица на новой строке под курсором. Пустые строки
  // до/после — не потому что parseTableAt их требует (не требует, таблица
  // парсится и вплотную к соседнему тексту), а чтобы вставка не сливалась
  // с текстом текущей строки, если курсор был не в её начале/конце.
  export function insertTable() {
    if (!view) return;
    const { state } = view;
    const pos = state.selection.main.head;
    const line = state.doc.lineAt(pos);
    const needsLeadingBlank = line.text.trim() !== "";
    const table = serializeTable(emptyTable(2, 2));
    const insert = (needsLeadingBlank ? "\n\n" : "\n") + table + "\n\n";
    view.dispatch({
      changes: { from: line.to, insert },
      selection: EditorSelection.cursor(line.to + insert.length),
      scrollIntoView: true,
    });
    view.focus();
  }
</script>

<div class="cm-host" bind:this={hostEl}></div>

<style>
  .cm-host {
    flex: 1;
    display: flex;
    overflow: hidden;
  }
  .cm-host :global(.cm-editor) {
    width: 100%;
  }
</style>
