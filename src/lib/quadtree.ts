// Barnes-Hut quadtree (v0.9.23) — приближённый расчёт отталкивания узлов в
// force-directed графе заметок (NotesGraph.svelte) за O(n log n) вместо
// точного O(n²) перебора всех пар на каждый кадр симуляции. Дальние группы
// узлов приближаются одной точкой с суммарной массой в её центре масс —
// чем дальше узел от группы (относительно её размера), тем грубее и дешевле
// приближение; theta регулирует этот компромисс точность/скорость.

export type QuadBody = { id: string; x: number; y: number; mass: number };

type Bounds = { x: number; y: number; size: number };

type QuadNode = {
  bounds: Bounds;
  // Лист хранит список тел, а не одно: на пределе глубины (совпадающие
  // координаты) деление больше не разносит тела по квадрантам, и они
  // обязаны копиться здесь — иначе их масса теряется, см. insert().
  bodies: QuadBody[];
  mass: number;
  cx: number; cy: number; // центр масс поддерева
  children: [QuadNode, QuadNode, QuadNode, QuadNode] | null; // NW NE SW SE
};

function quadrantOf(bounds: Bounds, x: number, y: number): 0 | 1 | 2 | 3 {
  const half = bounds.size / 2;
  const midX = bounds.x + half, midY = bounds.y + half;
  const east = x >= midX, south = y >= midY;
  if (!south && !east) return 0; // NW
  if (!south && east) return 1; // NE
  if (south && !east) return 2; // SW
  return 3; // SE
}

function childBounds(bounds: Bounds, quadrant: 0 | 1 | 2 | 3): Bounds {
  const half = bounds.size / 2;
  const x = bounds.x + (quadrant === 1 || quadrant === 3 ? half : 0);
  const y = bounds.y + (quadrant === 2 || quadrant === 3 ? half : 0);
  return { x, y, size: half };
}

const MAX_DEPTH = 24;

function insert(node: QuadNode, body: QuadBody, depth: number): void {
  if (node.children) {
    const q = quadrantOf(node.bounds, body.x, body.y);
    insert(node.children[q], body, depth + 1);
  } else if (node.bodies.length === 0 || depth >= MAX_DEPTH) {
    // Пустой лист — просто кладём тело. На пределе глубины (совпадающие или
    // почти совпадающие координаты) деление уже не разнесёт тела по разным
    // квадрантам и рекурсировало бы до переполнения стека, поэтому лист
    // становится «толстым»: копит несколько тел и сохраняет их массу.
    node.bodies.push(body);
  } else {
    const existing = node.bodies;
    node.bodies = [];
    node.children = [0, 1, 2, 3].map(q => makeLeaf(childBounds(node.bounds, q as 0 | 1 | 2 | 3))) as QuadNode["children"];
    for (const e of existing) {
      insert(node.children![quadrantOf(node.bounds, e.x, e.y)], e, depth + 1);
    }
    insert(node.children![quadrantOf(node.bounds, body.x, body.y)], body, depth + 1);
  }

  let mass = 0, cx = 0, cy = 0;
  if (node.children) {
    for (const c of node.children) {
      if (c.mass === 0) continue;
      mass += c.mass;
      cx += c.cx * c.mass;
      cy += c.cy * c.mass;
    }
  } else {
    for (const b of node.bodies) {
      mass += b.mass;
      cx += b.x * b.mass;
      cy += b.y * b.mass;
    }
  }
  node.mass = mass;
  node.cx = mass > 0 ? cx / mass : 0;
  node.cy = mass > 0 ? cy / mass : 0;
}

// Стабильный «случайный» угол из id — детерминированный, чтобы совпавшие
// узлы расталкивались одинаково от кадра к кадру, а не дрожали.
function hashAngle(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) | 0;
  return (h & 0xffff) / 0xffff * Math.PI * 2;
}

function makeLeaf(bounds: Bounds): QuadNode {
  return { bounds, bodies: [], mass: 0, cx: 0, cy: 0, children: null };
}

export class Quadtree {
  private root: QuadNode;

  constructor(bounds: Bounds, bodies: QuadBody[]) {
    this.root = makeLeaf(bounds);
    for (const b of bodies) insert(this.root, b, 0);
  }

  // Приближённая сила отталкивания на тело с координатами (x,y,excludeId),
  // по закону force = strength / distSq (та же формула, что была в прямом
  // O(n²) переборе) — theta контролирует точность/скорость приближения:
  // меньше theta -> точнее (ближе к прямому перебору), больше -> быстрее.
  repulsion(x: number, y: number, excludeId: string, strength: number, theta = 0.8): { fx: number; fy: number } {
    let fx = 0, fy = 0;
    const stack: QuadNode[] = [this.root];
    while (stack.length > 0) {
      const node = stack.pop()!;
      if (node.mass === 0) continue;

      if (!node.children) {
        // Лист: считаем каждое тело точно. Себя пропускаем, но соседей с
        // теми же координатами — нет, иначе «толстый» лист терял бы их вклад.
        for (const b of node.bodies) {
          if (b.id === excludeId) continue;
          let dx = x - b.x, dy = y - b.y;
          if (dx === 0 && dy === 0) {
            // Тела ровно друг на друге: сила есть, а направления нет —
            // (dx/dist) обнуляет обе оси и узлы залипают навсегда. Даём
            // детерминированный по id толчок, чтобы они разъехались.
            const a = hashAngle(b.id);
            dx = Math.cos(a) * 0.5;
            dy = Math.sin(a) * 0.5;
          }
          const distSq = Math.max(dx * dx + dy * dy, 1);
          const dist = Math.sqrt(distSq);
          const force = (strength * b.mass) / distSq;
          fx += (dx / dist) * force;
          fy += (dy / dist) * force;
        }
        continue;
      }

      const ddx = x - node.cx, ddy = y - node.cy;
      const ddistSq = Math.max(ddx * ddx + ddy * ddy, 1);
      // Barnes-Hut критерий: если узел квадродерева достаточно мал и далёк
      // (size/dist < theta), приближаем всё поддерево одной точкой массы.
      // Центр масс ровно в точке запроса — направления нет, приближать
      // нельзя (сила молча обнулится): спускаемся к листьям, там тела
      // разбираются поимённо и себя можно исключить точно.
      if (node.bounds.size / Math.sqrt(ddistSq) < theta && (ddx !== 0 || ddy !== 0)) {
        const dist = Math.sqrt(ddistSq);
        const force = (strength * node.mass) / ddistSq;
        fx += (ddx / dist) * force;
        fy += (ddy / dist) * force;
      } else {
        for (const c of node.children) stack.push(c);
      }
    }
    return { fx, fy };
  }
}

// Границы квадрата, накрывающего все точки с запасом — квадродерево требует
// квадратную (не прямоугольную) область, иначе quadrantOf делит неровно.
export function boundsFor(points: { x: number; y: number }[], padding = 50): Bounds {
  if (points.length === 0) return { x: 0, y: 0, size: 1 };
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const p of points) {
    if (p.x < minX) minX = p.x;
    if (p.y < minY) minY = p.y;
    if (p.x > maxX) maxX = p.x;
    if (p.y > maxY) maxY = p.y;
  }
  const size = Math.max(maxX - minX, maxY - minY, 1) + padding * 2;
  return { x: minX - padding, y: minY - padding, size };
}
