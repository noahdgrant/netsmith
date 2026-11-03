import { describe, expect, it } from 'vitest';
import { layoutGraph } from './graph';

const box = (id: string, w = 100, h = 50) => ({ id, w, h });

describe('layoutGraph', () => {
  it('puts a node one column right of what feeds it', () => {
    const l = layoutGraph([box('a'), box('b'), box('c')], [
      { from: 'a', to: 'b' },
      { from: 'b', to: 'c' },
    ]);
    const column = (id: string) => l.nodes.find((n) => n.id === id)!.column;
    expect([column('a'), column('b'), column('c')]).toEqual([0, 1, 2]);
  });

  it('uses the longest path, so a node fed twice sits right of both', () => {
    const l = layoutGraph([box('a'), box('b'), box('c')], [
      { from: 'a', to: 'b' },
      { from: 'b', to: 'c' },
      { from: 'a', to: 'c' },
    ]);
    expect(l.nodes.find((n) => n.id === 'c')!.column).toBe(2);
  });

  it('stacks nodes of a column without overlap and ends on a cycle', () => {
    const l = layoutGraph([box('a'), box('b'), box('c')], [
      { from: 'a', to: 'b' },
      { from: 'a', to: 'c' },
      { from: 'b', to: 'a' },
    ]);
    const [b, c] = ['b', 'c'].map((id) => l.nodes.find((n) => n.id === id)!);
    expect(b.column).toBe(c.column);
    expect(Math.abs(b.y - c.y)).toBeGreaterThanOrEqual(50);
    expect(l.width).toBeGreaterThan(0);
  });

  it('ignores a link to a node that is not there', () => {
    const l = layoutGraph([box('a')], [{ from: 'a', to: 'zzz' }]);
    expect(l.nodes).toHaveLength(1);
  });
});

describe('layoutGraph with many nodes and no links', () => {
  it('cuts a long column into lanes, so the whole is not a strip', () => {
    const boxes = Array.from({ length: 40 }, (_, i) => box(`n${i}`, 236, 200));
    const l = layoutGraph(boxes, []);
    expect(l.nodes).toHaveLength(40);
    expect(l.width / l.height).toBeGreaterThan(0.6);
    expect(l.width / l.height).toBeLessThan(3.2);
    // No two cards overlap.
    for (const a of l.nodes) {
      for (const b of l.nodes) {
        if (a === b) continue;
        const apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
        expect(apart).toBe(true);
      }
    }
  });
});
