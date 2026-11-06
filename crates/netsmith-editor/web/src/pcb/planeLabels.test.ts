import { describe, expect, it } from 'vitest';

import { type Box, INSET, LINE, firstFree, labelBox, meets, spots } from './planeLabels';

const box = (left: number, top: number, w: number, h: number): Box => ({ left, top, right: left + w, bottom: top + h });

describe('a plane label and the parts', () => {
  it('boxes that touch within the margin meet; apart they do not', () => {
    expect(meets(box(0, 0, 10, 10), box(11, 0, 10, 10), 2)).toBe(true);
    expect(meets(box(0, 0, 10, 10), box(13, 0, 10, 10), 2)).toBe(false);
    expect(meets(box(0, 0, 10, 10), box(5, 5, 2, 2))).toBe(true);
  });
  it('takes the first corner that is free, so a label stays where it was when nothing lies there', () => {
    const corners = [box(0, 0, 40, 10), box(160, 0, 40, 10), box(0, 90, 40, 10), box(160, 90, 40, 10)];
    expect(firstFree(corners, [])).toBe(0);
    expect(firstFree(corners, [box(10, 0, 20, 20)])).toBe(1);
    expect(firstFree(corners, [box(10, 0, 20, 20), box(150, 0, 30, 30)])).toBe(2);
    expect(firstFree(corners, [box(10, 0, 20, 20), box(150, 0, 30, 30), box(0, 85, 50, 15)])).toBe(3);
  });
  it('says there is none when a part (or an earlier label) lies in every corner', () => {
    const corners = [box(0, 0, 40, 10), box(160, 0, 40, 10)];
    expect(firstFree(corners, [box(0, 0, 200, 50)])).toBe(-1);
  });
  it('has four corners of the plane to try, top left first, the right ones anchored at their end', () => {
    const s = spots([10, 20, 100, 40], 0);
    expect(s.slice(0, 4).map((p) => p.anchor)).toEqual(['start', 'end', 'start', 'end']);
    expect(s[0]).toEqual({ x: 10 + INSET, y: 20 + INSET, anchor: 'start' });
    expect(s[3]).toEqual({ x: 10 + 100 - INSET, y: 20 + 40 - INSET, anchor: 'end' });
  });
  it('measures a label from its text and size: as tall as a line, as wide as its letters, growing to the left from a right corner', () => {
    const a = labelBox(100, 50, 10, 12, 'start');
    expect(a.right - a.left).toBeCloseTo(72);
    expect(a.bottom - a.top).toBeCloseTo(12);
    expect(a.left).toBe(100);
    expect((a.top + a.bottom) / 2).toBeCloseTo(50);
    const b = labelBox(100, 50, 10, 12, 'end');
    expect(b.right).toBe(100);
    expect(b.left).toBeCloseTo(28);
  });
  it('then the same corners a line and two lines further in, so a second label can sit under the first', () => {
    const s = spots([0, 0, 100, 40], 0);
    expect(s.length).toBeGreaterThanOrEqual(12);
    expect(s[5].y - s[1].y).toBeCloseTo(LINE);
    expect(s[10].y - s[6].y).toBeCloseTo(-LINE);
  });
  it('and last along the top and bottom edges, left to right, for a gap that is not at a corner', () => {
    const s = spots([0, 0, 100, 40], 0).slice(12);
    expect(s.length).toBeGreaterThan(10);
    expect(s.every((p) => p.anchor === 'start' && (p.y === INSET || p.y === 40 - INSET))).toBe(true);
    const tops = s.filter((p) => p.y === INSET).map((p) => p.x);
    expect([...tops].sort((a, b) => a - b)).toEqual(tops);
    expect(tops.at(-1)!).toBeLessThan(100 - INSET);
  });
  it('a label stacked under another in the first corner starts a line lower', () => {
    expect(spots([0, 0, 50, 50], 1)[0].y).toBeCloseTo(INSET + LINE);
    expect(spots([0, 0, 50, 50], 1)[1].y).toBeCloseTo(INSET);
  });
});
