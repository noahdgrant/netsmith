import { describe, expect, it } from 'vitest';

import { GRID, type Item, type Link, type Routed, type TreeLayout, layoutTree } from './layout';

const card = (id: string, h = 100, w = 260): Item => ({ id, kind: 'rail', w, h, anchor: 24 });
const small = (id: string): Item => ({ id, kind: 'stage', w: 180, h: 64, anchor: 32 });

/** Every straight stretch of every edge: [x1, y1, x2, y2]. */
const stretches = (e: Routed) => e.points.slice(1).map((p, i) => [e.points[i][0], e.points[i][1], p[0], p[1]] as const);

function through(l: TreeLayout): string[] {
  const bad: string[] = [];
  for (const e of l.edges) {
    for (const [x1, y1, x2, y2] of stretches(e)) {
      for (const n of l.nodes) {
        if (n.id === e.from || n.id === e.to) continue;
        const hit = Math.max(x1, x2) > n.x - 1 && Math.min(x1, x2) < n.x + n.w + 1 && Math.max(y1, y2) > n.y - 1 && Math.min(y1, y2) < n.y + n.h + 1;
        if (hit) bad.push(`${e.from}>${e.to} through ${n.id}`);
      }
    }
  }
  return bad;
}

/** A tree with a cycle (the third rail feeds the stage that feeds the first), a fan-out, a long edge. */
const items = [card('A'), small('s1'), card('B'), small('s2'), card('C', 200), small('s3'), card('D'), small('s4'), card('E'), small('back')];
const links: Link[] = [
  { from: 'A', to: 's1' },
  { from: 's1', to: 'B' },
  { from: 'B', to: 's2' },
  { from: 's2', to: 'C' },
  { from: 'C', to: 's3' },
  { from: 's3', to: 'D' },
  { from: 'C', to: 's4' },
  { from: 's4', to: 'E' },
  { from: 'A', to: 's4' },
  { from: 'C', to: 'back' },
  { from: 'back', to: 'A' },
];

describe('the flow layout', () => {
  const l = layoutTree(items, links);
  it('keeps every stretch of every edge, the ones that go back too, out of every unrelated node', () => {
    expect(l.edges).toHaveLength(links.length);
    expect(l.edges.filter((e) => e.back).length).toBeGreaterThan(0);
    expect(through(l)).toEqual([]);
  });
  it('puts the ends of the edges on a grid, so no jog is smaller than a step', () => {
    for (const e of l.edges) {
      for (const [x1, y1, x2, y2] of stretches(e)) {
        if (x1 === x2) {
          const len = Math.abs(y2 - y1);
          expect(len === 0 || len >= GRID, `${e.from}>${e.to}: a vertical run of ${len}`).toBe(true);
        }
      }
      expect(e.points[0][1] % GRID).toBe(0);
      expect(e.points[e.points.length - 1][1] % GRID).toBe(0);
    }
  });
  it('has no stretch shorter than 6 px', () => {
    for (const e of l.edges) for (const [x1, y1, x2, y2] of stretches(e)) expect(Math.hypot(x2 - x1, y2 - y1), `${e.from}>${e.to}`).toBeGreaterThanOrEqual(6);
  });
  it('draws a straight wire between a pair that can be aligned', () => {
    const e = layoutTree([card('A'), card('B')], [{ from: 'A', to: 'B' }]).edges[0];
    expect(new Set(e.points.map((p) => p[1])).size).toBe(1);
  });
  it('is the same every time, whatever order it is given', () => {
    const again = layoutTree([...items].reverse(), [...links].reverse());
    expect(again.edges.map((e) => e.d).sort()).toEqual(l.edges.map((e) => e.d).sort());
    expect(again.nodes.map((n) => [n.id, n.x, n.y]).sort()).toEqual(l.nodes.map((n) => [n.id, n.x, n.y]).sort());
  });
  it('sends an edge that goes back along the bottom, under every node', () => {
    const e = l.edges.find((q) => q.back)!;
    const lowest = Math.max(...l.nodes.map((n) => n.y + n.h));
    expect(Math.max(...e.points.map((p) => p[1]))).toBeGreaterThan(lowest);
    expect(e.points[0][0]).toBeGreaterThan(l.nodes.find((n) => n.id === e.from)!.x);
    expect(e.points[e.points.length - 1][0]).toBe(l.nodes.find((n) => n.id === e.to)!.x);
  });
  it('fits its own size', () => {
    for (const e of l.edges) for (const [x, y] of e.points) expect(x >= 0 && y >= 0 && x <= l.width && y <= l.height).toBe(true);
    for (const n of l.nodes) expect(n.x + n.w <= l.width && n.y + n.h <= l.height).toBe(true);
  });
  it('lays out the empty graph', () => {
    expect(layoutTree([], []).nodes).toEqual([]);
  });
});

describe('labels', () => {
  const labelled: Link[] = [
    { from: 'A', to: 'B', label: '+VBATT_SAFE', labelW: 90 },
    { from: 'A', to: 'C', label: 'SPI1_MOSI', labelW: 70 },
    { from: 'A', to: 'D', label: 'x', labelW: 12 },
    { from: 'B', to: 'D', label: 'UART0_TX', labelW: 64 },
  ];
  const ls = layoutTree([card('A', 140), card('B'), card('C'), card('D')], labelled);
  it('stands each on its edge, left of the edge\'s first turn', () => {
    for (const e of ls.edges) {
      expect(e.label).toBeDefined();
      const turn = e.points.find((p) => p[0] !== e.points[0][0] && p[1] !== e.points[0][1]);
      void turn;
      expect(e.label!.x + e.label!.w).toBeLessThanOrEqual(Math.max(...e.points.slice(1).map((p) => p[0])) + 1);
    }
  });
  it('never overlaps two labels, nor one with a node', () => {
    const boxes = ls.edges.map((e) => ({ x: e.label!.x, y: e.label!.y - 13, w: e.label!.w, h: 15, id: e.from + e.to }));
    for (const a of boxes) {
      for (const b of boxes) {
        if (a.id < b.id) expect(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y, `${a.id} on ${b.id}`).toBe(true);
      }
      for (const n of ls.nodes) expect(a.x + a.w <= n.x || n.x + n.w <= a.x || a.y + a.h <= n.y || n.y + n.h <= a.y, `${a.id} on ${n.id}`).toBe(true);
    }
  });
});
