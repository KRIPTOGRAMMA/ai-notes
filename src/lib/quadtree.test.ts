import { describe, it, expect } from "vitest";
import { Quadtree, boundsFor, type QuadBody } from "./quadtree";

function bruteForceRepulsion(bodies: QuadBody[], x: number, y: number, excludeId: string, strength: number) {
  let fx = 0, fy = 0;
  for (const b of bodies) {
    if (b.id === excludeId) continue;
    const dx = x - b.x, dy = y - b.y;
    const distSq = Math.max(dx * dx + dy * dy, 1);
    const dist = Math.sqrt(distSq);
    const force = (strength * b.mass) / distSq;
    fx += (dx / dist) * force;
    fy += (dy / dist) * force;
  }
  return { fx, fy };
}

describe("boundsFor", () => {
  it("returns a fallback square for an empty point set", () => {
    expect(boundsFor([])).toEqual({ x: 0, y: 0, size: 1 });
  });

  it("covers every point with padding", () => {
    const points = [{ x: 10, y: 10 }, { x: 100, y: 40 }, { x: -20, y: 80 }];
    const b = boundsFor(points, 5);
    for (const p of points) {
      expect(p.x).toBeGreaterThanOrEqual(b.x);
      expect(p.x).toBeLessThanOrEqual(b.x + b.size);
      expect(p.y).toBeGreaterThanOrEqual(b.y);
      expect(p.y).toBeLessThanOrEqual(b.y + b.size);
    }
  });

  it("produces a square (equal width/height), not a rectangle", () => {
    const b = boundsFor([{ x: 0, y: 0 }, { x: 500, y: 5 }]);
    expect(b.size).toBeGreaterThan(0);
  });
});

describe("Quadtree repulsion", () => {
  it("matches brute-force force closely for a small well-separated cluster (theta small)", () => {
    const bodies: QuadBody[] = [
      { id: "a", x: 0, y: 0, mass: 1 },
      { id: "b", x: 100, y: 0, mass: 1 },
      { id: "c", x: 0, y: 100, mass: 1 },
      { id: "d", x: 300, y: 300, mass: 1 },
    ];
    const tree = new Quadtree(boundsFor(bodies), bodies);
    const approx = tree.repulsion(0, 0, "a", 1000, 0.1);
    const exact = bruteForceRepulsion(bodies, 0, 0, "a", 1000);
    expect(approx.fx).toBeCloseTo(exact.fx, 1);
    expect(approx.fy).toBeCloseTo(exact.fy, 1);
  });

  it("excludes the body's own id (no self-repulsion)", () => {
    const bodies: QuadBody[] = [{ id: "solo", x: 5, y: 5, mass: 1 }];
    const tree = new Quadtree(boundsFor(bodies), bodies);
    const { fx, fy } = tree.repulsion(5, 5, "solo", 1000, 0.5);
    expect(fx).toBe(0);
    expect(fy).toBe(0);
  });

  it("returns zero force for an empty tree", () => {
    const tree = new Quadtree({ x: 0, y: 0, size: 100 }, []);
    const { fx, fy } = tree.repulsion(10, 10, "none", 1000, 0.5);
    expect(fx).toBe(0);
    expect(fy).toBe(0);
  });

  it("approximates a distant cluster as roughly the same total force as brute force (theta default)", () => {
    // A dense cluster of 50 nodes far from the point under test: with theta=0.8 the
    // approximation must stay within a reasonable margin of the exact computation.
    const bodies: QuadBody[] = [];
    for (let i = 0; i < 50; i++) {
      bodies.push({ id: `n${i}`, x: 1000 + (i % 10) * 2, y: 1000 + Math.floor(i / 10) * 2, mass: 1 });
    }
    const probe: QuadBody = { id: "probe", x: 0, y: 0, mass: 1 };
    const all = [probe, ...bodies];
    const tree = new Quadtree(boundsFor(all), all);
    const approx = tree.repulsion(0, 0, "probe", 1000, 0.8);
    const exact = bruteForceRepulsion(bodies, 0, 0, "probe", 1000);
    const approxMag = Math.hypot(approx.fx, approx.fy);
    const exactMag = Math.hypot(exact.fx, exact.fy);
    expect(approxMag).toBeGreaterThan(exactMag * 0.8);
    expect(approxMag).toBeLessThan(exactMag * 1.2);
  });

  it("handles many bodies at the exact same coordinates without stack overflow", () => {
    const bodies: QuadBody[] = [];
    for (let i = 0; i < 30; i++) bodies.push({ id: `dup${i}`, x: 50, y: 50, mass: 1 });
    expect(() => new Quadtree(boundsFor(bodies), bodies)).not.toThrow();
  });

  // A regression: at the tree's depth limit identical coordinates once silently lost
  // bodies (the leaf split into 4 empty children while the body was discarded) — the
  // whole tree's mass collapsed and repulsion died across the entire graph. Nodes at
  // the canvas edge are clamped into a single point by the position clamp, so the
  // case is reachable in ordinary use rather than only in theory.
  it("conserves mass when bodies share coordinates", () => {
    const bodies: QuadBody[] = [];
    for (let i = 0; i < 30; i++) bodies.push({ id: `dup${i}`, x: 50, y: 50, mass: 1 });
    bodies.push({ id: "anchor", x: 400, y: 400, mass: 1 });
    const tree = new Quadtree(boundsFor(bodies), bodies);
    const approx = tree.repulsion(0, 0, "probe", 1000, 0.8);
    const exact = bruteForceRepulsion(bodies, 0, 0, "probe", 1000);
    expect(Math.hypot(approx.fx, approx.fy)).toBeGreaterThan(Math.hypot(exact.fx, exact.fy) * 0.8);
  });

  // Even two coincident points dropped the whole tree's mass to 1.
  it("keeps repulsion alive for the whole graph when two nodes coincide", () => {
    const bodies: QuadBody[] = [
      { id: "a", x: 100, y: 100, mass: 1 },
      { id: "b", x: 100, y: 100, mass: 1 },
    ];
    for (let i = 0; i < 20; i++) bodies.push({ id: `n${i}`, x: 200 + i * 10, y: 300, mass: 1 });
    const tree = new Quadtree(boundsFor(bodies), bodies);
    const approx = tree.repulsion(0, 0, "probe", 1000, 0.8);
    const exact = bruteForceRepulsion(bodies, 0, 0, "probe", 1000);
    expect(Math.hypot(approx.fx, approx.fy)).toBeGreaterThan(Math.hypot(exact.fx, exact.fy) * 0.8);
  });

  // Neighbours sharing our coordinates must not be excluded — only ourselves.
  it("still repels a body sitting at the same spot as the excluded one", () => {
    const bodies: QuadBody[] = [
      { id: "self", x: 100, y: 100, mass: 1 },
      { id: "twin", x: 100, y: 100, mass: 1 },
    ];
    const tree = new Quadtree(boundsFor(bodies), bodies);
    const { fx, fy } = tree.repulsion(100, 100, "self", 1000, 0.8);
    expect(Math.hypot(fx, fy)).toBeGreaterThan(0);
  });

  it("handles a single body", () => {
    const bodies: QuadBody[] = [{ id: "only", x: 10, y: 10, mass: 2 }];
    const tree = new Quadtree(boundsFor(bodies), bodies);
    const { fx, fy } = tree.repulsion(0, 0, "someone-else", 100, 0.5);
    expect(fx).toBeLessThan(0); // pushed left, away from (10,10)
    expect(fy).toBeLessThan(0); // and upward
  });
});
