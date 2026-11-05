import { describe, expect, it } from 'vitest';

import {
  K100,
  MAX_COMFORT_K,
  MIN_LEGIBLE_K,
  centreOnBox,
  fitWidthView,
  isHeldForText,
  kOfPercent,
  percentLabel,
  percentOfK,
  placeSheet,
  revealBox,
  textPx,
  zoomAbout,
  zoomRange,
} from './zoom';

describe('percent', () => {
  it('is the drawn size at 96 dpi: a millimetre is 3.78 px at 100%', () => {
    // 6 canvas pixels to a millimetre.
    expect(K100 * 6).toBeCloseTo(3.7795, 3);
    expect(percentLabel(K100)).toBe('100%');
    expect(percentOfK(kOfPercent(250))).toBeCloseTo(250);
  });
  it('gives the reference text 4.8 px at 100% and 7 px at the smallest that reads', () => {
    expect(textPx(K100)).toBeCloseTo(4.8, 1);
    expect(textPx(MIN_LEGIBLE_K)).toBeCloseTo(7);
    expect(percentLabel(MIN_LEGIBLE_K)).toBe('146%');
  });
});

describe('placeSheet', () => {
  // A 100 mm by 70 mm sheet is 600 by 420 canvas pixels.
  const small = { x: 0, y: 0, w: 600, h: 420 };
  it('fits a sheet whose text is readable when it is fitted', () => {
    const p = placeSheet(1000, 700, small);
    expect(p.mode).toBe('fit');
    expect(textPx(p.view.k)).toBeGreaterThanOrEqual(7);
    expect(p.view.k).toBeCloseTo(p.fitK);
  });
  it('opens a wide sheet at the smallest scale that reads, at its top left, not fitted', () => {
    // 522 mm by 98 mm: fitted to 1630 px the text is 3.8 px.
    const wide = { x: 40, y: 30, w: 522 * 6, h: 98 * 6 };
    const p = placeSheet(1630, 940, wide, 16);
    expect(p.mode).toBe('legible');
    expect(textPx(p.fitK)).toBeLessThan(7);
    expect(textPx(p.view.k)).toBeCloseTo(7);
    // The top left of the box is 16 px from the top left of the window.
    expect(wide.x * p.view.k + p.view.x).toBeCloseTo(16);
    expect(wide.y * p.view.k + p.view.y).toBeCloseTo(16);
  });
  it('does not blow a small sheet up past a comfortable size, and centres it', () => {
    const tiny = { x: 0, y: 0, w: 40 * 6, h: 30 * 6 };
    const p = placeSheet(1630, 940, tiny);
    expect(p.mode).toBe('comfort');
    expect(p.view.k).toBeCloseTo(MAX_COMFORT_K);
    expect(textPx(p.view.k)).toBeCloseTo(16);
    expect(tiny.w * p.view.k / 2 + p.view.x).toBeCloseTo(1630 / 2);
  });
  it('gives the same text size to every sheet that fits within the bounds: no 7x spread', () => {
    const sizes = [
      { x: 0, y: 0, w: 66 * 6, h: 41 * 6 }, // gnss
      { x: 0, y: 0, w: 170 * 6, h: 85 * 6 }, // reg3v8
      { x: 0, y: 0, w: 522 * 6, h: 98 * 6 }, // modem
      { x: 0, y: 0, w: 121 * 6, h: 208 * 6 }, // mcu
    ].map((b) => textPx(placeSheet(1630, 940, b).view.k));
    expect(Math.min(...sizes)).toBeGreaterThanOrEqual(7 - 1e-9);
    expect(Math.max(...sizes) / Math.min(...sizes)).toBeLessThan(2.4);
  });
});

describe('isHeldForText', () => {
  it('is true only where the sheet would not read fitted and the zoom is the one that reads', () => {
    expect(isHeldForText(MIN_LEGIBLE_K / 2, MIN_LEGIBLE_K)).toBe(true);
    expect(isHeldForText(MIN_LEGIBLE_K / 2, MIN_LEGIBLE_K * 3)).toBe(false);
    expect(isHeldForText(MIN_LEGIBLE_K * 2, MIN_LEGIBLE_K)).toBe(false);
  });
});

describe('fitWidthView', () => {
  it('makes the box as wide as the window with its top at the top', () => {
    const v = fitWidthView(1000, { x: 100, y: 50, w: 500, h: 900 }, 1, 16);
    expect(v.k).toBe(2);
    expect((100 + 500) * v.k + v.x).toBeCloseTo(1000);
    expect(100 * v.k + v.x).toBeCloseTo(0);
    expect(50 * v.k + v.y).toBeCloseTo(16);
  });
});

describe('zoomAbout and zoomRange', () => {
  it('keeps the point under the pointer where it is', () => {
    const v = { k: 1, x: 10, y: 20 };
    const z = zoomAbout(v, 2, 300, 200);
    expect((300 - v.x) / v.k).toBeCloseTo((300 - z.x) / z.k);
    expect((200 - v.y) / v.k).toBeCloseTo((200 - z.y) / z.k);
  });
  it('lets a person zoom out a little past fit and in to 2400%', () => {
    const r = zoomRange(kOfPercent(60));
    expect(percentOfK(r.min)).toBeCloseTo(10);
    expect(percentOfK(r.max)).toBeCloseTo(2400);
    expect(zoomRange(kOfPercent(12)).min).toBeCloseTo(kOfPercent(6));
  });
});

describe('centreOnBox and revealBox', () => {
  const box = { x: 1000, y: 500, w: 100, h: 60 };
  it('puts the middle of the box in the middle of the window and raises a small zoom to the legible one', () => {
    const v = centreOnBox(800, 600, box, { k: 0.2, x: 0, y: 0 }, { minK: MIN_LEGIBLE_K });
    expect(v.k).toBeCloseTo(MIN_LEGIBLE_K);
    expect(1050 * v.k + v.x).toBeCloseTo(400);
    expect(530 * v.k + v.y).toBeCloseTo(300);
  });
  it('keeps the zoom when it already reads', () => {
    const v = centreOnBox(800, 600, box, { k: 2, x: 0, y: 0 }, { minK: MIN_LEGIBLE_K });
    expect(v.k).toBe(2);
  });
  it('moves no more than it must to bring a box in, and not at all when it is in', () => {
    const here = { k: 1, x: 0, y: 0 };
    expect(revealBox(800, 600, { x: 300, y: 200, w: 20, h: 20 }, here, 0)).toEqual(here);
    const v = revealBox(800, 600, { x: 900, y: 200, w: 20, h: 20 }, here, 0, 40);
    expect(v.y).toBe(0);
    expect(920 + v.x).toBeCloseTo(760);
  });
});
