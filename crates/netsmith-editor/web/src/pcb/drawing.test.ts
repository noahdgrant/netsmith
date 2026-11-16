import { describe, expect, it } from 'vitest';

import { stackedLabels } from './drawing';

describe('the labels of height limits that start at one corner', () => {
  it('stay where they are when no two meet', () => {
    expect(stackedLabels([{ x: 1, y: 1 }, { x: 20, y: 1 }, { x: 1, y: 9 }])).toEqual([1, 1, 9]);
  });

  it('go a line below each other, so a floor clearance over the whole board and a lower limit at its edge are both readable', () => {
    expect(stackedLabels([{ x: 1, y: 1 }, { x: 1.2, y: 1 }, { x: 1, y: 1 }])).toEqual([1, 1.8, 2.6]);
  });

  it('do not move for a label far to the side, or lower than a line', () => {
    expect(stackedLabels([{ x: 1, y: 1 }, { x: 9, y: 1 }, { x: 1, y: 3 }])).toEqual([1, 1, 3]);
  });
});
