<script lang="ts">
  // v0.9.01: граф заметок — force-directed визуализация вики-связей.
  // Узлы = заметки, рёбра = [[wikilinks]] (extractWikiLinks, тот же парсер,
  // что и бэклинки в Notes.svelte). Без внешней библиотеки — своя простая
  // симуляция (repulsion между всеми узлами + attraction вдоль рёбер +
  // притяжение к центру), рендер в SVG. Изолированные заметки (без связей)
  // остаются в графе, но приглушены и оттеснены к краям через более слабое
  // центральное притяжение.
  import { untrack } from "svelte";
  import { noteStore } from "../lib/stores/notes.svelte";
  import { projectStore } from "../lib/stores/projects.svelte";
  import { extractWikiLinks } from "../lib/markdown";
  import { Quadtree, boundsFor } from "../lib/quadtree";
  import type { Note } from "../lib/types";

  import { t } from "../lib/i18n.svelte";
  let { onOpenNote }: { onOpenNote: (id: string) => void } = $props();

  type GNode = {
    id: string;
    title: string;
    x: number; y: number;
    vx: number; vy: number;
    degree: number;
    color: string | null;
  };
  type GEdge = { source: string; target: string };

  let width = $state(800);
  let height = $state(600);
  let container: HTMLDivElement = $state()!;

  const projectColor = $derived.by(() => {
    const m = new Map<string, string>();
    for (const p of projectStore.projects) m.set(p.id, p.color);
    return m;
  });

  // Граф пересчитывается только когда реально меняется состав заметок/связей
  // (не на каждый кадр симуляции — позиции живут отдельно в nodes-массиве).
  const { nodes, edges } = $derived.by(() => {
    const notes = noteStore.notes;
    const byTitle = new Map<string, Note>();
    for (const n of notes) byTitle.set(n.title.trim().toLowerCase(), n);

    const edges: GEdge[] = [];
    const seenPair = new Set<string>();
    const degree = new Map<string, number>();
    for (const n of notes) {
      for (const link of extractWikiLinks(n.content)) {
        const target = byTitle.get(link.trim().toLowerCase());
        if (!target || target.id === n.id) continue;
        const pair = [n.id, target.id].sort().join("|");
        if (seenPair.has(pair)) continue;
        seenPair.add(pair);
        edges.push({ source: n.id, target: target.id });
        degree.set(n.id, (degree.get(n.id) ?? 0) + 1);
        degree.set(target.id, (degree.get(target.id) ?? 0) + 1);
      }
    }

    const nodes: GNode[] = notes.map((n, i) => {
      const angle = (i / Math.max(notes.length, 1)) * Math.PI * 2;
      const r = Math.min(width, height) * 0.35;
      return {
        id: n.id,
        title: n.title,
        x: width / 2 + Math.cos(angle) * r,
        y: height / 2 + Math.sin(angle) * r,
        vx: 0, vy: 0,
        degree: degree.get(n.id) ?? 0,
        color: n.project_id ? (projectColor.get(n.project_id) ?? null) : null,
      };
    });
    return { nodes, edges };
  });

  // Позиции живут в отдельном reactive-массиве, обновляемом тиками симуляции
  // (не пересоздаётся с $derived nodes, иначе drag/simulation сбрасывались бы
  // при каждой правке любой заметки).
  let positions = $state<Map<string, { x: number; y: number; vx: number; vy: number }>>(new Map());
  let draggingId: string | null = $state(null);

  // Читает `nodes` реактивно (пересчёт при смене состава заметок), но
  // `positions` — через untrack, иначе эффект читает и пишет одно и то же
  // состояние и Svelte уходит в effect_update_depth_exceeded (бесконечный
  // цикл на каждое обновление стора, даже нерелевантное графу).
  $effect(() => {
    const ids = nodes.map(n => n.id);
    untrack(() => {
      const next = new Map(positions);
      let changed = false;
      for (const n of nodes) {
        if (!next.has(n.id)) {
          next.set(n.id, { x: n.x, y: n.y, vx: 0, vy: 0 });
          changed = true;
        }
      }
      for (const id of [...next.keys()]) {
        if (!ids.includes(id)) {
          next.delete(id);
          changed = true;
        }
      }
      if (changed) {
        positions = next;
        wake(); // состав графа изменился (новая/удалённая заметка) — досчитать layout
      }
    });
  });

  let rafId: number | null = null;
  function tick() {
    const pos = positions;

    // Позиция перетаскиваемого узла применяется ровно один раз за кадр,
    // а не на каждое событие указателя.
    if (draggingId && pendingDrag) {
      const dp = pos.get(draggingId);
      if (dp) {
        dp.x = pendingDrag.x;
        dp.y = pendingDrag.y;
        dp.vx = 0; dp.vy = 0;
      }
      pendingDrag = null;
    }

    const REPULSION = 6000;
    const SPRING = 0.08;
    const SPRING_LEN = 110;
    const CENTER_PULL = 0.02;
    const ISOLATED_CENTER_PULL = 0.003; // приглушённые узлы слабее тянутся к центру -> к краям
    const DAMPING = 0.6;

    // Barnes-Hut (v0.9.23): раньше здесь был точный O(n²) перебор всех пар
    // узлов на каждый кадр — на графах в несколько сотен заметок это душило
    // CPU и заметно тормозило симуляцию. Квадродерево строится заново каждый
    // кадр (позиции меняются), но сам расчёт отталкивания на узел падает до
    // O(log n) вместо O(n) — итого O(n log n) на кадр вместо O(n²).
    const bodies = [...pos.entries()].map(([id, p]) => ({ id, x: p.x, y: p.y, mass: 1 }));
    const tree = new Quadtree(boundsFor(bodies), bodies);

    for (const n of nodes) {
      const p = pos.get(n.id);
      if (!p || draggingId === n.id) continue;

      const { fx: rfx, fy: rfy } = tree.repulsion(p.x, p.y, n.id, REPULSION);
      let fx = rfx, fy = rfy;

      const pull = n.degree > 0 ? CENTER_PULL : ISOLATED_CENTER_PULL;
      fx += (width / 2 - p.x) * pull;
      fy += (height / 2 - p.y) * pull;

      p.vx = (p.vx + fx) * DAMPING;
      p.vy = (p.vy + fy) * DAMPING;
    }

    for (const e of edges) {
      const sp = pos.get(e.source), tp = pos.get(e.target);
      if (!sp || !tp) continue;
      const dx = tp.x - sp.x, dy = tp.y - sp.y;
      const dist = Math.max(Math.sqrt(dx * dx + dy * dy), 1);
      const stretch = dist - SPRING_LEN;
      const fx = (dx / dist) * stretch * SPRING;
      const fy = (dy / dist) * stretch * SPRING;
      if (draggingId !== e.source) { sp.vx += fx; sp.vy += fy; }
      if (draggingId !== e.target) { tp.vx -= fx; tp.vy -= fy; }
    }

    let totalMotion = 0;
    for (const [id, p] of pos) {
      if (draggingId === id) continue;
      p.x += p.vx;
      p.y += p.vy;
      p.x = Math.max(20, Math.min(width - 20, p.x));
      p.y = Math.max(20, Math.min(height - 20, p.y));
      totalMotion += Math.abs(p.vx) + Math.abs(p.vy);
    }

    positions = new Map(pos);
    // Симуляция «остывает»: как только суммарное движение узлов падает ниже
    // порога, останавливаем RAF-цикл — иначе граф дёргается бесконечно (лишняя
    // нагрузка на CPU и невозможно надёжно кликнуть по узлу в e2e). Драг узла
    // или изменение состава графа снова запускают цикл (см. $effect ниже).
    // Пока узел тянут, цикл не останавливаем: движение перетаскиваемого узла
    // не учитывается в totalMotion, и граф «остыл» бы прямо под курсором.
    if (draggingId !== null || totalMotion > 0.05 * nodes.length) {
      rafId = requestAnimationFrame(tick);
    } else {
      rafId = null;
    }
  }

  function wake() {
    if (rafId === null) rafId = requestAnimationFrame(tick);
  }

  $effect(() => {
    rafId = requestAnimationFrame(tick);
    return () => { if (rafId) cancelAnimationFrame(rafId); };
  });

  $effect(() => {
    if (!container) return;
    const ro = new ResizeObserver(entries => {
      const e = entries[0];
      if (!e) return;
      width = e.contentRect.width;
      height = e.contentRect.height;
    });
    ro.observe(container);
    return () => ro.disconnect();
  });

  // Рект контейнера кэшируется на время перетаскивания: getBoundingClientRect()
  // на каждый pointermove — синхронный layout flush, а WebKitGTK шлёт эти
  // события ~170/сек. Во время драга контейнер не двигается, читать его
  // заново незачем.
  let dragRect: DOMRect | null = null;
  // Последняя позиция указателя, применяется один раз за кадр в tick():
  // раньше каждый pointermove клонировал всю Map позиций ради реактивности,
  // то есть ~170 клонов в секунду вместо 60.
  let pendingDrag: { x: number; y: number } | null = null;

  function startDrag(id: string, e: PointerEvent) {
    draggingId = id;
    dragRect = container.getBoundingClientRect();
    (e.target as Element).setPointerCapture(e.pointerId);
    wake(); // тянут узел за другими остывшими — снова нужно пересчитывать соседей
  }
  function onDrag(id: string, e: PointerEvent) {
    if (draggingId !== id || !dragRect) return;
    pendingDrag = { x: e.clientX - dragRect.left, y: e.clientY - dragRect.top };
    wake();
  }
  function endDrag() {
    draggingId = null;
    dragRect = null;
    pendingDrag = null;
  }

  let hoveredId: string | null = $state(null);
  const connectedIds = $derived.by(() => {
    if (!hoveredId) return null;
    const s = new Set<string>([hoveredId]);
    for (const e of edges) {
      if (e.source === hoveredId) s.add(e.target);
      if (e.target === hoveredId) s.add(e.source);
    }
    return s;
  });
</script>

<div class="graph-view">
  <div class="graph-header">
    <h2>{t("Граф заметок")}</h2>
    <span class="muted">{t("{n} заметок", { n: nodes.length })} · {t("{n} связей", { n: edges.length })}</span>
  </div>

  {#if nodes.length === 0}
    <p class="empty muted">{t("Пока нет заметок — граф появится, когда будут заметки со связями [[как эта]].")}</p>
  {:else}
    <div class="canvas" bind:this={container}>
      <svg {width} {height}>
        <g class="edges">
          {#each edges as e (e.source + "|" + e.target)}
            {@const sp = positions.get(e.source)}
            {@const tp = positions.get(e.target)}
            {#if sp && tp}
              <line
                x1={sp.x} y1={sp.y} x2={tp.x} y2={tp.y}
                class="edge"
                class:dim={connectedIds && !(connectedIds.has(e.source) && connectedIds.has(e.target))}
              />
            {/if}
          {/each}
        </g>
        <g class="nodes">
          {#each nodes as n (n.id)}
            {@const p = positions.get(n.id)}
            {#if p}
              <g
                class="node"
                class:isolated={n.degree === 0}
                class:dim={connectedIds && !connectedIds.has(n.id)}
                transform="translate({p.x},{p.y})"
                onpointerdown={(e) => startDrag(n.id, e)}
                onpointermove={(e) => onDrag(n.id, e)}
                onpointerup={endDrag}
                onpointerleave={() => { if (draggingId !== n.id) hoveredId = null; }}
                onpointerenter={() => hoveredId = n.id}
                ondblclick={() => onOpenNote(n.id)}
                role="button"
                tabindex="0"
              >
                <!-- Невидимый увеличенный хитбокс: покрывает круг + подпись одним
                     сплошным прямоугольником, иначе клик мимо пикселей круга/текста
                     (в промежутке) не попадает по <g> — сложно кликнуть и ненадёжно
                     в e2e (elementFromPoint промахивается на пустое место внутри bbox). -->
                <rect x="-10" y="-10" width={100} height="20" fill="transparent" />
                <circle
                  r={n.degree === 0 ? 5 : 6 + Math.min(n.degree, 8)}
                  fill={n.color ?? (n.degree === 0 ? "var(--text-secondary)" : "var(--accent)")}
                />
                <!-- Плашка-подложка под подписью вместо stroke-обводки текста
                     (v0.9.23) — WebKitGTK перерисовывает stroke-контур текста
                     заново на каждый кадр симуляции (SVG не умеет частичную
                     перерисовку), это заметно дороже одного залитого rect. -->
                <rect class="label-bg" x="7" y="-6" width={n.title.length * 6 + 6} height="12" rx="2" />
                <text x="10" y="4">{n.title}</text>
              </g>
            {/if}
          {/each}
        </g>
      </svg>
    </div>
    <p class="hint muted">{t("Перетаскивайте узлы, двойной клик — открыть заметку. Приглушённые узлы без связей.")}</p>
  {/if}
</div>

<style>
  .graph-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 8px;
  }

  .graph-header {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .graph-header h2 {
    margin: 0;
    font-size: 16px;
  }

  .muted {
    color: var(--text-secondary);
    font-size: 12px;
  }

  .empty {
    padding: 24px;
  }

  .canvas {
    flex: 1;
    min-height: 300px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
    overflow: hidden;
  }

  svg {
    display: block;
  }

  .edge {
    stroke: var(--border);
    stroke-width: 1.4;
    transition: opacity .15s;
  }

  .edge.dim {
    opacity: .15;
  }

  .node {
    cursor: grab;
    transition: opacity .15s;
  }

  .node.isolated circle {
    opacity: .5;
  }

  .node.dim {
    opacity: .25;
  }

  .node text {
    font-size: 11px;
    fill: var(--text-primary);
    pointer-events: none;
    user-select: none;
  }

  .label-bg {
    fill: var(--bg-secondary);
    opacity: .85;
    pointer-events: none;
  }

  .hint {
    text-align: center;
  }
</style>
