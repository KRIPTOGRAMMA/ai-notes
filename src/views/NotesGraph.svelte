<script lang="ts">
  // The notes graph: a force-directed visualization of wiki links. Nodes are
  // notes, edges are [[wikilinks]] (via extractWikiLinks, the same parser that
  // powers backlinks in Notes.svelte). No external library — a simple simulation of
  // our own (repulsion between all nodes, attraction along edges, and a pull
  // towards the centre), rendered as SVG. Isolated notes with no links stay in the
  // graph but are dimmed and pushed towards the edges by a weaker centre pull.
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

  // The graph is recomputed only when the set of notes or links actually changes,
  // not on every simulation frame — the positions live separately from the nodes.
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

  // The positions live in their own reactive map, updated by the simulation ticks
  // (not recreated along with the $derived nodes, or a drag and the simulation
  // would reset on every edit to any note).
  let positions = $state<Map<string, { x: number; y: number; vx: number; vy: number }>>(new Map());
  let draggingId: string | null = $state(null);

  // Reads `nodes` reactively (recomputing when the set of notes changes) but
  // `positions` through untrack: otherwise the effect reads and writes the same
  // state and Svelte hits effect_update_depth_exceeded — an infinite loop on every
  // store update, even ones irrelevant to the graph.
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
        // The coordinates live outside reactivity (see paintFrame), so freshly
        // mounted nodes have no attributes yet and without a first paint they would
        // all pile up at 0,0. We wait for tick(): by the next frame Svelte has
        // mounted the elements and bind:this has filled the references, whereas here,
        // before the DOM update, they are still null.
        wake(); // the graph's contents changed (a note added or removed) — finish the layout
      }
    });
  });

  let rafId: number | null = null;

  // References to the live SVG elements. paintFrame() writes the coordinates into
  // them itself, bypassing Svelte's reactivity — see the explanation above it.
  let nodeEls: Record<string, SVGGElement | null> = {};
  let edgeEls: Record<string, SVGLineElement | null> = {};

  // This, not the frame rate, was the real cause of the stuttering.
  //
  // The markup used to read coordinates through {@const p = positions.get(n.id)}
  // while the tick ended with `positions = new Map(pos)`. But that is a SHALLOW
  // copy: the new Map holds the very same {x,y,vx,vy} objects, and the physics
  // mutates them in place (p.x += dx). Svelte sees the assignment to positions but
  // finds the same objects inside, considers them already read, and does not update
  // the markup. So during a drag, when only the coordinates inside an object
  // change, the DOM was not touched at all: the node stood still under the cursor,
  // and on release draggingId changed a $state, a genuine re-render ran, and the
  // node jumped to where it had long been.
  //
  // We write into the attributes directly. That also removes Svelte re-rendering
  // the whole SVG 72 times a second: per frame we touch exactly n nodes and m edges.
  function paintFrame(pos: Map<string, { x: number; y: number }>) {
    for (const n of nodes) {
      const p = pos.get(n.id);
      const el = nodeEls[n.id];
      if (p && el) el.setAttribute("transform", `translate(${p.x},${p.y})`);
    }
    for (const e of edges) {
      const el = edgeEls[e.source + "|" + e.target];
      const sp = pos.get(e.source), tp = pos.get(e.target);
      if (!el || !sp || !tp) continue;
      el.setAttribute("x1", String(sp.x));
      el.setAttribute("y1", String(sp.y));
      el.setAttribute("x2", String(tp.x));
      el.setAttribute("y2", String(tp.y));
    }
  }

  function tick() {
    const pos = positions;

    // The dragged node's position is applied exactly once per frame rather than on
    // every pointer event.
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
    const ISOLATED_CENTER_PULL = 0.003; // dimmed nodes are pulled to the centre less, so they drift outward
    const DAMPING = 0.6;

    // Barnes-Hut: this used to be an exact O(n²) pass over every pair of nodes on
    // every frame, which on graphs of several hundred notes throttled the CPU and
    // visibly slowed the simulation. The quadtree is rebuilt each frame (the
    // positions change), but computing repulsion per node drops to O(log n) instead
    // of O(n) — O(n log n) per frame rather than O(n²).
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

    paintFrame(pos);
    // The simulation "cools down": as soon as the total motion of the nodes falls
    // below a threshold we stop the RAF loop, or the graph would twitch forever
    // (wasted CPU, and no way to click a node reliably in e2e). Dragging a node or
    // changing the graph's contents starts the loop again (see the $effect below).
    // While a node is being dragged the loop is not stopped: the dragged node's
    // motion is excluded from totalMotion, so the graph would "cool" right under
    // the cursor.
    if (draggingId !== null || totalMotion > 0.05 * nodes.length) {
      rafId = requestAnimationFrame(tick);
    } else {
      rafId = null;
    }
  }

  function wake() {
    if (rafId === null) rafId = requestAnimationFrame(tick);
  }

  // Cancelling the previous frame is mandatory: the effect re-runs, and without it
  // it simply overwrote rafId and left the old loop orphaned. Measurements caught
  // 137-145 ticks/s on a 72Hz screen — exactly double, two loops running in
  // parallel, with half the physics and rendering going to waste.
  $effect(() => {
    if (rafId !== null) cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(tick);
    return () => {
      if (rafId !== null) { cancelAnimationFrame(rafId); rafId = null; }
      // Leaving the section mid-drag would leave the listeners on window.
      window.removeEventListener("pointermove", onWindowDrag);
      window.removeEventListener("pointerup", endDrag);
    };
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

  // The container's rect is cached for the duration of a drag:
  // getBoundingClientRect() on every pointermove is a synchronous layout flush, and
  // WebKitGTK delivers those events at about 170/s. The container does not move
  // during a drag, so re-reading it is pointless.
  let dragRect: DOMRect | null = null;
  // The last pointer position, applied once per frame in tick(): every pointermove
  // used to clone the whole positions Map for reactivity's sake, i.e. about 170
  // clones a second instead of 60.
  let pendingDrag: { x: number; y: number } | null = null;

  function startDrag(id: string, e: PointerEvent) {
    draggingId = id;
    dragRect = container.getBoundingClientRect();
    // Pointer capture is not used here, and would be harmful: the listeners on
    // window already catch the cursor anywhere, while a capture redirects subsequent
    // events to the capturing element — which is why a dblclick on a node stopped
    // reaching it.
    //
    // Capturing on e.target was doubly wrong: the target is a child <circle> or
    // <rect> inside the node, which Svelte re-rendered on every position update, so
    // the capture died with the old element. The instruments showed the price:
    // pointermove fell from about 170/s to 14-19/s, meaning the node received a new
    // position 15 times a second — exactly the "15 fps by eye", even though the
    // frames themselves ran at a healthy 72.
    window.addEventListener("pointermove", onWindowDrag);
    window.addEventListener("pointerup", endDrag);
    wake(); // a node is dragged among cooled-down ones, so the neighbours need recomputing
  }

  // The listener sits on window rather than on the node: while the cursor outruns
  // the node no events occur over it at all and the position stops updating.
  function onWindowDrag(e: PointerEvent) {
    if (!draggingId || !dragRect) return;
    pendingDrag = { x: e.clientX - dragRect.left, y: e.clientY - dragRect.top };
    wake();
  }
  function endDrag() {
    draggingId = null;
    dragRect = null;
    pendingDrag = null;
    window.removeEventListener("pointermove", onWindowDrag);
    window.removeEventListener("pointerup", endDrag);
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
          <!-- paintFrame() writes the coordinates here directly via setAttribute:
               Svelte's reactivity does not work on them (see the comment there). -->
          {#each edges as e (e.source + "|" + e.target)}
            <line
              bind:this={edgeEls[e.source + "|" + e.target]}
              class="edge"
              class:dim={connectedIds && !(connectedIds.has(e.source) && connectedIds.has(e.target))}
            />
          {/each}
        </g>
        <g class="nodes">
          {#each nodes as n (n.id)}
              <g
                bind:this={nodeEls[n.id]}
                class="node"
                class:isolated={n.degree === 0}
                class:dim={connectedIds && !connectedIds.has(n.id)}
                onpointerdown={(e) => startDrag(n.id, e)}
                onpointerup={endDrag}
                onpointerleave={() => { if (draggingId !== n.id) hoveredId = null; }}
                onpointerenter={() => hoveredId = n.id}
                ondblclick={() => onOpenNote(n.id)}
                role="button"
                tabindex="0"
              >
                <!-- An invisible enlarged hitbox covering the circle and the label with one
                     solid rectangle. Without it a click that misses the pixels of the
                     circle or the text (landing in the gap between them) does not hit
                     the <g>: hard to click and unreliable in e2e, where
                     elementFromPoint lands on empty space inside the bbox. -->
                <rect x="-10" y="-10" width={100} height="20" fill="transparent" />
                <circle
                  r={n.degree === 0 ? 5 : 6 + Math.min(n.degree, 8)}
                  fill={n.color ?? (n.degree === 0 ? "var(--text-secondary)" : "var(--accent)")}
                />
                <!-- A backing plate under the label instead of a stroke outline on
                     the text: WebKitGTK redraws a text stroke from scratch on every
                     simulation frame (SVG cannot repaint partially), which is
                     noticeably more expensive than one filled rect. -->
                <rect class="label-bg" x="7" y="-6" width={n.title.length * 6 + 6} height="12" rx="2" />
                <text x="10" y="4">{n.title}</text>
              </g>
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
