import { describe, expect, it } from 'vitest';

import { type Box, INSET, LINE, type Placeable, firstFree, labelBox, meets, gridSpots, moreTag, placeAll, spots } from './planeLabels';

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

describe('placing every label of a drawing', () => {
  // A plane is `x y w h` in millimetres; one millimetre is `px` screen pixels here, a label 60 by 12 of them.
  const px = 10;
  const item = (id: string, plane: [number, number, number, number], stacked = 0, w = 60, h = 12): Placeable<string> => ({
    label: id,
    spots: spots(plane, stacked),
    boxAt: (spot) => ({ left: spot.anchor === 'start' ? spot.x * px : spot.x * px - w, top: spot.y * px - h / 2, right: spot.anchor === 'start' ? spot.x * px + w : spot.x * px, bottom: spot.y * px + h / 2 }),
  });
  const plane: [number, number, number, number] = [0, 0, 100, 50];
  const meetAny = (list: (Box | null)[]) => list.some((a, i) => a && list.some((b, j) => i !== j && b && meets(a, b, 2)));

  it('puts labels of planes that share their corners apart: the second goes to the next place, a line in', () => {
    const { chosen, boxes } = placeAll([item('a', plane), item('b', plane), item('c', plane)], []);
    expect(chosen[0]).toBe(0);
    expect(chosen.every((i) => i >= 0)).toBe(true);
    expect(new Set(chosen).size).toBe(3);
    expect(meetAny(boxes)).toBe(false);
  });
  it('keeps a label off a part, whatever its first corner was', () => {
    const part: Box = { left: 0, top: 0, right: 300, bottom: 40 };
    const { chosen, boxes } = placeAll([item('a', plane)], [part]);
    expect(chosen[0]).toBeGreaterThan(0);
    expect(meets(boxes[0]!, part, 2)).toBe(false);
  });
  it('says -1 for a label with no place, and does not let a later one take its turn out of order', () => {
    // a block over the whole plane: nothing is free
    const { chosen, boxes } = placeAll([item('a', plane), item('b', plane)], [{ left: -50, top: -50, right: 2000, bottom: 2000 }]);
    expect(chosen).toEqual([-1, -1]);
    expect(boxes).toEqual([null, null]);
  });
  it('never makes two labels meet, for any number of planes in any corner (a seeded shuffle of 200 boards)', () => {
    let seed = 7;
    const rnd = () => ((seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296);
    for (let n = 0; n < 200; n++) {
      const items: Placeable<string>[] = [];
      for (let k = 0; k < 1 + Math.floor(rnd() * 5); k++) {
        const w = 20 + rnd() * 80;
        const h = 10 + rnd() * 40;
        items.push(item(`p${k}`, [rnd() * 20, rnd() * 20, w, h], Math.floor(rnd() * 3), 30 + rnd() * 50));
      }
      const blocks: Box[] = Array.from({ length: Math.floor(rnd() * 4) }, () => {
        const left = rnd() * 900;
        const top = rnd() * 400;
        return { left, top, right: left + 20 + rnd() * 200, bottom: top + 20 + rnd() * 120 };
      });
      const { chosen, boxes } = placeAll(items, blocks);
      expect(meetAny(boxes), `board ${n}`).toBe(false);
      boxes.forEach((b, i) => {
        if (b) for (const blk of blocks) expect(meets(b, blk, 2), `board ${n} label ${i} on a block`).toBe(false);
        expect(chosen[i] >= 0).toBe(b !== null);
      });
    }
  });
  it('a label that is placed stays where an earlier one left room, so the order of the labels is the order of the planes', () => {
    const a = placeAll([item('a', plane), item('b', plane)], []);
    const b = placeAll([item('b', plane), item('a', plane)], []);
    expect(a.chosen[0]).toBe(b.chosen[0]);
    expect(a.chosen[0]).toBe(0);
  });
});

describe('the tag for labels that found no place', () => {
  it('says how many and lists them, one to a line', () => {
    expect(moreTag(['GND \u00b7 In1.Cu', '3V3 \u00b7 In2.Cu'])).toEqual({ text: '+2', title: 'GND \u00b7 In1.Cu\n3V3 \u00b7 In2.Cu' });
    expect(moreTag(['GND \u00b7 In1.Cu'])?.text).toBe('+1');
  });
  it('is none when every label has its place', () => {
    expect(moreTag([])).toBeNull();
  });
});

describe('places for the tag anywhere in a plane', () => {
  it('are a grid inside the border, row by row from the top, none outside the plane', () => {
    const g = gridSpots([10, 20, 50, 30], 2);
    expect(g.length).toBeGreaterThan(50);
    expect(g.every((p) => p.x >= 10 + INSET && p.x < 10 + 50 - INSET && p.y > 20 + INSET && p.y < 20 + 30 - INSET)).toBe(true);
    const ys = g.map((p) => p.y);
    expect([...ys].sort((a, b) => a - b)).toEqual(ys);
    expect(g[0].anchor).toBe('start');
  });
  it('is nothing for a plane too small to hold one', () => {
    expect(gridSpots([0, 0, 1.5, 1.5], 2)).toEqual([]);
  });
});
