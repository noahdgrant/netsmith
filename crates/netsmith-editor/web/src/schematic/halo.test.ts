import { describe, expect, it } from 'vitest';

import { HALO_PAD, MARK_PAD, padBox, unionBoxes } from './halo';

describe('the halo of a part', () => {
  it('is the union of the boxes of the body, and nothing when there are none', () => {
    expect(unionBoxes([])).toBeNull();
    expect(unionBoxes([{ x: 1, y: 1, width: 0, height: 0 }])).toBeNull();
    expect(unionBoxes([{ x: 0, y: 0, width: 2, height: 1 }, { x: 5, y: -3, width: 1, height: 2 }])).toEqual({ x: 0, y: -3, width: 6, height: 4 });
    // a line has no width but has a length
    expect(unionBoxes([{ x: 3, y: 0, width: 0, height: 4 }])).toEqual({ x: 3, y: 0, width: 0, height: 4 });
  });
  it('grows around the body, keeps its centre and is at least as wide as asked', () => {
    expect(padBox({ x: 10, y: 20, width: 4, height: 2 }, 1)).toEqual({ x: 9, y: 19, width: 6, height: 4 });
    const thin = padBox({ x: 10, y: 20, width: 0, height: 6 }, 1, 4);
    expect(thin).toEqual({ x: 8, y: 19, width: 4, height: 8 });
    expect(thin.x + thin.width / 2).toBe(10);
  });
  it('has the comparison’s mark outside the pick’s halo', () => {
    expect(MARK_PAD).toBeGreaterThan(HALO_PAD);
  });
});
