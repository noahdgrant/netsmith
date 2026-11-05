import { describe, expect, it } from 'vitest';

import { DOCK_DEFAULT, DOCK_MIN, clampDock, parseDock } from './dock';

describe('clampDock', () => {
  it('keeps the dock between its least and 70% of the window', () => {
    expect(clampDock(10, 1000)).toBe(DOCK_MIN);
    expect(clampDock(300, 1000)).toBe(300);
    expect(clampDock(900, 1000)).toBe(700);
    expect(clampDock(900, 100)).toBe(DOCK_MIN);
    expect(clampDock(Number.NaN, 1000)).toBe(DOCK_DEFAULT);
  });
});

describe('parseDock', () => {
  it('takes what was kept and ignores what is not usable', () => {
    expect(parseDock({ open: true, height: 333 })).toEqual({ open: true, height: 333 });
    expect(parseDock(null)).toEqual({ open: false, height: DOCK_DEFAULT });
    expect(parseDock({ open: 'yes', height: 'tall' })).toEqual({ open: false, height: DOCK_DEFAULT });
    expect(parseDock({ open: true, height: 5 }).height).toBe(DOCK_MIN);
  });
});
