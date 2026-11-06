import { describe, expect, it } from 'vitest';

import { showsAll } from './MiniMap';

const size = { width: 1000, height: 400 };
describe('showsAll: no overview while the window shows the whole drawing', () => {
  it('is false while there is no view', () => expect(showsAll(size, null)).toBe(false));
  it('is true when the drawing fits and sits inside the window', () => {
    expect(showsAll(size, { k: 1, x: 20, y: 30, w: 1280, h: 700 })).toBe(true); // drawn 20 px in: nothing is lost
    expect(showsAll(size, { k: 1, x: -20, y: 30, w: 1280, h: 700 })).toBe(false); // the left edge is off the window
    expect(showsAll(size, { k: 1, x: 0, y: 0, w: 1280, h: 700 })).toBe(true);
  });
  it('is true when the drawing is fitted with a margin', () => {
    expect(showsAll(size, { k: 0.5, x: 0, y: 0, w: 600, h: 300 })).toBe(true);
  });
  it('is false once zoomed in so that part is outside', () => {
    expect(showsAll(size, { k: 2, x: 0, y: 0, w: 1280, h: 700 })).toBe(false);
    expect(showsAll(size, { k: 1, x: -200, y: 0, w: 1280, h: 700 })).toBe(false);
  });
});
