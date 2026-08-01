// A Barnes-Hut quadtree: an approximate computation of node repulsion in the
// force-directed notes graph (NotesGraph.svelte) in O(n log n) instead of an exact
// O(n²) pass over every pair on each simulation frame. Distant groups of nodes are
// approximated by a single point carrying their total mass at the centre of mass —
// the further a node is from a group (relative to the group's size), the coarser
// and cheaper the approximation; theta governs that accuracy/speed trade-off.

export type QuadBody = { id: string; x: number; y: number; mass: number };

type Bounds = { x: number; y: number; size: number };

type QuadNode = {
  bounds: Bounds;
  // A leaf holds a list of bodies rather than one: at the depth limit (identical
  // coordinates) subdividing no longer separates bodies into quadrants, so they must
  // accumulate here — otherwise their mass is lost, see insert().
  bodies: QuadBody[];
  mass: number;
  cx: number; cy: number; // the subtree's centre of mass
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
    // An empty leaf simply takes the body. At the depth limit (identical or nearly
    // identical coordinates) subdividing would no longer separate the bodies into
    // different quadrants and would recurse until the stack overflowed, so the leaf
    // becomes "fat": it accumulates several bodies and preserves their mass.
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

// A stable "random" angle derived from the id — deterministic, so coincident nodes
// are pushed apart the same way from frame to frame instead of jittering.
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

  // The approximate repulsion force on a body at (x, y, excludeId), following
  // force = strength / distSq (the same formula the direct O(n²) pass used). theta
  // controls the approximation's accuracy versus speed: a smaller theta is more
  // accurate (closer to the direct pass), a larger one is faster.
  repulsion(x: number, y: number, excludeId: string, strength: number, theta = 0.8): { fx: number; fy: number } {
    let fx = 0, fy = 0;
    const stack: QuadNode[] = [this.root];
    while (stack.length > 0) {
      const node = stack.pop()!;
      if (node.mass === 0) continue;

      if (!node.children) {
        // A leaf: every body is computed exactly. We skip ourselves but not the
        // neighbours sharing our coordinates, or a "fat" leaf would lose their
        // contribution.
        for (const b of node.bodies) {
          if (b.id === excludeId) continue;
          let dx = x - b.x, dy = y - b.y;
          if (dx === 0 && dy === 0) {
            // Bodies exactly on top of one another: there is a force but no
            // direction — (dx/dist) zeroes both axes and the nodes stick together
            // forever. We give them a nudge derived deterministically from the id so
            // they drift apart.
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
      // The Barnes-Hut criterion: if a quadtree node is small and distant enough
      // (size/dist < theta), the whole subtree is approximated by a single point mass.
      // When the centre of mass sits exactly at the query point there is no direction
      // and approximating is not allowed (the force would silently zero out), so we
      // descend to the leaves, where the bodies are handled individually and we can
      // exclude ourselves exactly.
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

// The bounds of a square covering all the points with room to spare: a quadtree
// requires a square (not rectangular) region, or quadrantOf divides it unevenly.
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
