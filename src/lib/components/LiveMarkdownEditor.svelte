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
  import { EditorState, type Extension } from "@codemirror/state";
  import {
    EditorView, Decoration, type DecorationSet, WidgetType, keymap, ViewPlugin, type ViewUpdate,
    drawSelection, dropCursor, placeholder as cmPlaceholder,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { markdown } from "@codemirror/lang-markdown";
  import {
    autocompletion, completionKeymap,
    type CompletionContext, type CompletionResult,
  } from "@codemirror/autocomplete";

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

  function headingLevel(text: string): number {
    const m = /^(#{1,6})\s/.exec(text);
    return m ? m[1].length : 0;
  }

  // Строит decoration-набор для всего документа: строка с курсором показывает
  // сырой markdown (но только пока редактор реально в фокусе — иначе после
  // программной подмены value/пересинхронизации курсор на строке 1 навсегда
  // прятал бы виджеты в однострочных заметках), остальные — отрендеренный вид.
  function buildDecorations(state: EditorState, hasFocus: boolean): DecorationSet {
    const cursorLine = hasFocus ? state.doc.lineAt(state.selection.main.head).number : -1;
    // Decoration.set() сортирует сам (в отличие от ручного RangeSetBuilder,
    // где порядок на равном `from` легко перепутать между line/mark/replace).
    const items: { from: number; to: number; deco: Decoration }[] = [];

    for (let i = 1; i <= state.doc.lines; i++) {
      const line = state.doc.line(i);
      const raw = i === cursorLine;
      const text = line.text;

      // Заголовки: строку целиком метим классом размера, маркер '#' скрываем
      const hLevel = headingLevel(text);
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

      // Чекбоксы: "- [ ] " / "- [x] " → виджет, независимо от raw/rendered
      // (переключать чекбокс удобно в любом состоянии строки)
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

      if (!raw) {
        // Жирный **text** и курсив *text*/_text_ — маркеры скрываем, текст красим классом
        for (const m of text.matchAll(/\*\*([^*\n]+)\*\*/g)) {
          const from = line.from + m.index!;
          const to = from + m[0].length;
          items.push({ from, to: from + 2, deco: Decoration.replace({}) });
          items.push({ from: from + 2, to: to - 2, deco: Decoration.mark({ class: "cm-strong" }) });
          items.push({ from: to - 2, to, deco: Decoration.replace({}) });
        }
        for (const m of text.matchAll(/(?<!\*)\*([^*\n]+)\*(?!\*)|(?<!_)_([^_\n]+)_(?!_)/g)) {
          const from = line.from + m.index!;
          const to = from + m[0].length;
          items.push({ from, to: from + 1, deco: Decoration.replace({}) });
          items.push({ from: from + 1, to: to - 1, deco: Decoration.mark({ class: "cm-em" }) });
          items.push({ from: to - 1, to, deco: Decoration.replace({}) });
        }
        // Инлайн-код `code`
        for (const m of text.matchAll(/`([^`\n]+)`/g)) {
          const from = line.from + m.index!;
          const to = from + m[0].length;
          items.push({ from, to: from + 1, deco: Decoration.replace({}) });
          items.push({ from: from + 1, to: to - 1, deco: Decoration.mark({ class: "cm-code" }) });
          items.push({ from: to - 1, to, deco: Decoration.replace({}) });
        }
        // Вики-ссылки [[target]] / [[target|label]] → кликабельный виджет
        for (const m of text.matchAll(/\[\[([^\[\]|]+)(?:\|([^\[\]]+))?\]\]/g)) {
          const from = line.from + m.index!;
          const to = from + m[0].length;
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

    return Decoration.set(
      items.map(it => it.deco.range(it.from, it.to)),
      true, // sort
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
  });

  onMount(() => {
    if (!hostEl) return;
    const extensions: Extension[] = [
      history(),
      drawSelection(),
      dropCursor(),
      markdown(),
      livePreviewPlugin,
      autocompletion({ override: [wikiLinkCompletion] }),
      keymap.of([
        {
          key: "Mod-Enter",
          run: () => { onSubmitShortcut?.(); return true; },
        },
        ...historyKeymap,
        ...completionKeymap,
        ...defaultKeymap,
      ]),
      theme,
      EditorView.lineWrapping,
      cmPlaceholder(placeholderText),
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
