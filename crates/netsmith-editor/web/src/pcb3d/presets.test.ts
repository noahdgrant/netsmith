import { describe, expect, it } from 'vitest';

import { type Vec3, PRESETS, distanceFor, easeOut, fitPoints, flipped, halfHeightAt, orbit, presetDirection, presetOf, slerpDir } from './presets';

const close = (a: Vec3, b: Vec3, eps = 1e-6) => a.every((v, i) => Math.abs(v - b[i]) < eps);
const len = (v: Vec3) => Math.hypot(...v);

describe('the preset views', () => {
  it('every preset is a unit direction and is found again from it', () => {
    for (const { id } of PRESETS) {
      const d = presetDirection(id);
      expect(len(d)).toBeCloseTo(1, 9);
      expect(presetOf(d)).toBe(id);
    }
    expect(presetOf([0.3, 0.5, -0.8])).toBeNull();
  });
  it('top looks down and bottom up, each a hair off the pole toward the side that keeps the top edge up', () => {
    const top = presetDirection('top');
    const bottom = presetDirection('bottom');
    expect(top[1]).toBeGreaterThan(0.9999);
    expect(top[2]).toBeGreaterThan(0);
    expect(bottom[1]).toBeLessThan(-0.9999);
    expect(bottom[2]).toBeLessThan(0);
  });
  it('the sides look along the board: front from +Z (the bottom edge), right from +X', () => {
    expect(presetDirection('front')).toEqual([0, 0, 1]);
    expect(presetDirection('back')).toEqual([0, 0, -1]);
    expect(presetDirection('right')).toEqual([1, 0, 0]);
    expect(presetDirection('left')).toEqual([-1, 0, 0]);
    const iso = presetDirection('iso');
    expect(iso.every((v) => v > 0)).toBe(true);
  });
  it('the flip shows the other face: top and bottom swap, a view from above goes below', () => {
    expect(close(flipped(presetDirection('top')), presetDirection('bottom'))).toBe(true);
    expect(close(flipped(presetDirection('bottom')), presetDirection('top'))).toBe(true);
    const iso = presetDirection('iso');
    expect(flipped(iso)).toEqual([iso[0], -iso[1], iso[2]]);
  });
});

describe('fitting the board in the window', () => {
  const box: Vec3[] = [];
  for (const x of [-20, 20]) for (const y of [-1, 5]) for (const z of [-12, 12]) box.push([x, y, z]);
  it('from the top, the width or the height of the board decides, whichever fills the window first', () => {
    const wide = fitPoints(box, presetDirection('top'), 30, 2, 1);
    const tall = fitPoints(box, presetDirection('top'), 30, 0.5, 1);
    const tv = Math.tan((15 * Math.PI) / 180);
    // A wide window: the 24 mm of the board's height decide, seen from the top of the parts (5 mm up).
    expect(wide.distance).toBeCloseTo(5 + 12 / tv, 2);
    // A narrow one: the 40 mm of its width.
    expect(tall.distance).toBeCloseTo(5 + 20 / (tv * 0.5), 2);
    expect(wide.halfHeight).toBeCloseTo(12, 3);
    expect(tall.halfHeight).toBeCloseTo(40, 3);
  });
  it('every corner is inside the view at the distance found', () => {
    for (const { id } of PRESETS) {
      for (const aspect of [0.6, 1, 1.8]) {
        const dir = presetDirection(id);
        const { distance } = fitPoints(box, dir, 30, aspect);
        const cam: Vec3 = [dir[0] * distance, dir[1] * distance, dir[2] * distance];
        for (const p of box) {
          const to: Vec3 = [p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]];
          const along = -(to[0] * dir[0] + to[1] * dir[1] + to[2] * dir[2]);
          expect(along).toBeGreaterThan(0);
          const off = Math.sqrt(Math.max(0, len(to) ** 2 - along ** 2));
          // Within the larger of the two half angles (a cone round the frustum).
          const half = Math.atan(Math.tan((15 * Math.PI) / 180) * Math.max(1, aspect));
          expect(Math.atan2(off, along)).toBeLessThanOrEqual(half * Math.SQRT2 + 1e-9);
        }
      }
    }
  });
  it('a perspective and an orthographic camera show the same at the target', () => {
    expect(distanceFor(halfHeightAt(120, 30), 30)).toBeCloseTo(120, 9);
  });
});

describe('moving the camera', () => {
  it('an arrow key turns the view about the vertical and toward the pole, and stops short of it', () => {
    const d = orbit([0, 0, 1], Math.PI / 2, 0);
    expect(close(d, [1, 0, 0], 1e-9)).toBe(true);
    const up = orbit([0, 0, 1], 0, -Math.PI);
    expect(up[1]).toBeLessThan(1);
    expect(up[1]).toBeGreaterThan(0.99);
  });
  it('a move eases out and follows the great circle', () => {
    expect(easeOut(0)).toBe(0);
    expect(easeOut(1)).toBe(1);
    expect(easeOut(0.5)).toBeGreaterThan(0.5);
    const mid = slerpDir([1, 0, 0], [0, 1, 0], 0.5);
    expect(close(mid, [Math.SQRT1_2, Math.SQRT1_2, 0], 1e-9)).toBe(true);
    for (let t = 0; t <= 1; t += 0.125) expect(len(slerpDir(presetDirection('top'), presetDirection('bottom'), t))).toBeCloseTo(1, 9);
  });
});
