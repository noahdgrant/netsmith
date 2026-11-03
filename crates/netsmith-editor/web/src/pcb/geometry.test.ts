import { describe, expect, it } from 'vitest';
import {
  boxFrom,
  handlesOf,
  hitBox,
  hitHandle,
  inBox,
  normAngle,
  partTransform,
  resizeBox,
  rotateAbout,
  snap,
  snapMove,
  unite,
} from './geometry';

const box = (x: number, y: number, width: number, height: number) => ({ x, y, width, height });

describe('snap', () => {
  it('rounds to the grid, and a grid of 0 leaves the value', () => {
    expect(snap(1.26, 0.5)).toBe(1.5);
    expect(snap(1.24, 0.5)).toBe(1);
    expect(snap(1.2345, 0)).toBe(1.2345);
  });

  it('moves a selection so its anchor lands on the grid', () => {
    const d = snapMove({ x: 10.3, y: 4.1 }, { x: 2.0, y: 0.2 }, 1);
    expect(d.x).toBeCloseTo(12 - 10.3);
    expect(d.y).toBeCloseTo(4 - 4.1);
  });
});

describe('rotation', () => {
  it('turns counter-clockwise on screen: a point to the right goes up', () => {
    const p = rotateAbout({ x: 11, y: 5 }, { x: 10, y: 5 }, 90);
    expect(p.x).toBeCloseTo(10);
    expect(p.y).toBeCloseTo(4);
  });

  it('four quarter turns come back', () => {
    let p = { x: 3, y: 7 };
    for (let i = 0; i < 4; i++) p = rotateAbout(p, { x: 1, y: 1 }, 90);
    expect(p.x).toBeCloseTo(3);
    expect(p.y).toBeCloseTo(7);
  });

  it('keeps angles in 0..360', () => {
    expect(normAngle(-90)).toBe(270);
    expect(normAngle(450)).toBe(90);
    expect(normAngle(0)).toBe(0);
  });
});

describe('picking', () => {
  const big = { courtyard: box(0, 0, 10, 10), id: 'big' };
  const small = { courtyard: box(4, 4, 2, 2), id: 'small' };

  it('picks the smallest courtyard under the point', () => {
    expect(hitBox([big, small], { x: 5, y: 5 })?.id).toBe('small');
    expect(hitBox([big, small], { x: 1, y: 1 })?.id).toBe('big');
    expect(hitBox([big, small], { x: 20, y: 1 })).toBeNull();
  });

  it('selects what a box touches', () => {
    expect(inBox([big, small], box(5, 5, 20, 20)).map((t) => t.id)).toEqual(['big', 'small']);
    expect(inBox([big, small], box(11, 0, 3, 3))).toEqual([]);
  });

  it('draws a box from two corners in any order', () => {
    expect(boxFrom({ x: 5, y: 1 }, { x: 2, y: 4 })).toEqual(box(2, 1, 3, 3));
    expect(unite([box(0, 0, 1, 1), box(4, 5, 2, 2)])).toEqual(box(0, 0, 6, 7));
    expect(unite([])).toBeNull();
  });
});

describe('resizing a box', () => {
  const b = box(10, 10, 20, 10);

  it('moves the edges the handle holds and keeps the others', () => {
    expect(resizeBox(b, 'e', { x: 3, y: 9 })).toEqual(box(10, 10, 23, 10));
    expect(resizeBox(b, 'nw', { x: 2, y: 1 })).toEqual(box(12, 11, 18, 9));
    expect(resizeBox(b, 's', { x: 5, y: -4 })).toEqual(box(10, 10, 20, 6));
  });

  it('does not turn inside out: it stops at the least size', () => {
    const r = resizeBox(b, 'e', { x: -100, y: 0 }, 0.5);
    expect(r).toEqual(box(10, 10, 0.5, 10));
    const l = resizeBox(b, 'w', { x: 100, y: 0 }, 0.5);
    expect(l.x + l.width).toBe(30);
    expect(l.width).toBe(0.5);
  });

  it('finds the handle under the point, the nearest within reach', () => {
    expect(handlesOf(b)).toHaveLength(8);
    expect(hitHandle(b, { x: 30.2, y: 20.1 }, 1)).toBe('se');
    expect(hitHandle(b, { x: 20, y: 10.3 }, 1)).toBe('n');
    expect(hitHandle(b, { x: 20, y: 15 }, 1)).toBeNull();
  });
});

describe('the part transform', () => {
  it('is what the server draws', () => {
    expect(partTransform(5, 6, 0, false)).toBe('translate(5 6)');
    expect(partTransform(5, 6, 90, false)).toBe('translate(5 6) rotate(-90)');
    expect(partTransform(5, 6, 270, true)).toBe('translate(5 6) rotate(-270) scale(-1 1)');
  });
});
