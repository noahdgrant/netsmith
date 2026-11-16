import { describe, expect, it } from 'vitest';

import type { Sheet } from './schematic/drawing';
import { setOut } from './schematic/setout';
import { SCALE } from './schematic/zoom';

const sheet = (name: string, x: number, y: number): Sheet => ({ name, title: name, x: x * SCALE, y: y * SCALE, w: 40 * SCALE, h: 30 * SCALE });

describe('setOut', () => {
  it('orders the sheets row by row, each row from the left', () => {
    // Listed in source order; the drawing set them out b, c, a in the first row and d below.
    const sheets = [sheet('a', 105.41, 0), sheet('b', 0, 1.27), sheet('c', 52.07, 0.4), sheet('d', 0, 43.18)];
    expect(setOut(sheets, (s) => s).map((s) => s.name)).toEqual(['b', 'c', 'a', 'd']);
  });

  it('leaves what it is given as it was', () => {
    const sheets = [sheet('a', 50, 0), sheet('b', 0, 0)];
    setOut(sheets, (s) => s);
    expect(sheets.map((s) => s.name)).toEqual(['a', 'b']);
  });
});
