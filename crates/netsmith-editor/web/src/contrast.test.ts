import { describe, expect, it } from 'vitest';

import { contrast, luminance, over, parseHex } from './contrast';

describe('contrast', () => {
  it('is 21 for black on white and 1 for a colour on itself', () => {
    expect(contrast('#000', '#ffffff')).toBeCloseTo(21, 5);
    expect(contrast('#336699', '#336699')).toBeCloseTo(1, 5);
  });
  it('matches the published ratio of #767676 on white (4.54)', () => {
    expect(contrast('#767676', '#fff')!).toBeCloseTo(4.54, 2);
  });
  it('does not care which colour is first', () => {
    expect(contrast('#e8873a', '#1c1d20')).toBe(contrast('#1c1d20', '#e8873a'));
  });
  it('is null for a colour that is not hex', () => {
    expect(contrast('var(--x)', '#fff')).toBeNull();
    expect(parseHex('#12')).toBeNull();
  });
  it('reads short hex', () => {
    expect(parseHex('#fa0')).toEqual([255, 170, 0]);
    expect(luminance([255, 255, 255])).toBeCloseTo(1, 5);
  });
  it('mixes a translucent colour over a surface', () => {
    expect(over([255, 255, 255], [0, 0, 0], 0.5)).toEqual([128, 128, 128]);
  });
});
