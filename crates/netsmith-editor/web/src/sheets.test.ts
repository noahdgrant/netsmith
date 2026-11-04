import { describe, expect, it } from 'vitest';

import { ALL, ViewMemo, canStep, entries, fitView, resolvePick, stepPick } from './sheets';

const names = ['ldo', 'lights', 'pads'];

describe('resolvePick', () => {
  it('opens the first sheet of a board that has several', () => {
    expect(resolvePick(null, names)).toBe('ldo');
  });
  it('keeps a saved sheet that still exists, and the overview', () => {
    expect(resolvePick('pads', names)).toBe('pads');
    expect(resolvePick(ALL, names)).toBe(ALL);
  });
  it('forgets a sheet that is gone', () => {
    expect(resolvePick('gone', names)).toBe('ldo');
  });
  it('shows a board with one sheet, or none, as it is', () => {
    expect(resolvePick(null, ['only'])).toBe('only');
    expect(resolvePick(ALL, ['only'])).toBe('only');
    expect(resolvePick('x', [])).toBe(ALL);
  });
});

describe('stepPick', () => {
  it('goes through the overview and the sheets, and stops at the ends', () => {
    expect(entries(names)).toEqual([ALL, 'ldo', 'lights', 'pads']);
    expect(stepPick(names, 'ldo', 1)).toBe('lights');
    expect(stepPick(names, 'ldo', -1)).toBe(ALL);
    expect(stepPick(names, ALL, -1)).toBe(ALL);
    expect(stepPick(names, 'pads', 1)).toBe('pads');
    expect(stepPick(names, 'gone', 1)).toBe(ALL);
  });
  it('says which buttons work', () => {
    expect(canStep(names, ALL)).toEqual({ prev: false, next: true });
    expect(canStep(names, 'pads')).toEqual({ prev: true, next: false });
  });
});

describe('fitView', () => {
  it('fits a box to the width of a box that is taller than it needs', () => {
    const v = fitView(1000, 800, { x: 100, y: 50, w: 400, h: 100 }, 1);
    expect(v.k).toBe(2.5);
    // The box's left edge lands at the left of the view and its middle in the middle.
    expect(100 * v.k + v.x).toBeCloseTo(0);
    expect((50 + 50) * v.k + v.y).toBeCloseTo(400);
  });
});

describe('ViewMemo', () => {
  it('remembers a view for each key', () => {
    const m = new ViewMemo();
    m.set('b|ldo', { k: 2, x: 1, y: 2 });
    m.set('b|pads', { k: 3, x: 0, y: 0 });
    expect(m.get('b|ldo')).toEqual({ k: 2, x: 1, y: 2 });
    expect(m.get('b|none')).toBeUndefined();
  });
  it('forgets by prefix', () => {
    const m = new ViewMemo();
    m.set('b|ldo', { k: 2, x: 1, y: 2 });
    m.set('c|ldo', { k: 3, x: 0, y: 0 });
    m.forget('b|');
    expect(m.get('b|ldo')).toBeUndefined();
    expect(m.get('c|ldo')).toBeDefined();
  });
});
