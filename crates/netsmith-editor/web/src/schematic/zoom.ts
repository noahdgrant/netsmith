// Zoom for the schematic, in units a person can read: pure functions.
//
// The drawing is in millimetres, the canvas has `SCALE` pixels to each. "100%" is the sheet at
// its drawn size on a 96 dpi screen (1 mm = 3.78 px), so the number means the same on every
// sheet. Text in the drawing is 1.27 mm high, which is 4.8 px at 100%: too small to read on a
// screen, so a sheet that would fit only that small opens at the smallest scale that reads.

import type { ViewState } from '../sheets';
import { fitView } from '../sheets';

/** Canvas pixels in the page for each millimetre of the drawing. */
export const SCALE = 6;
/** Screen pixels in a millimetre at 96 dpi. */
export const PX_PER_MM = 96 / 25.4;
/** The height of the drawing's reference and value text, in millimetres. */
export const TEXT_MM = 1.27;
/** The smallest reference and value text that is still read, in screen pixels. */
export const MIN_TEXT_PX = 7;
/** The largest the text is made on its own: a small sheet is not blown up past this. */
export const MAX_TEXT_PX = 16;

/** The zoom (canvas pixel to screen pixel) at which the drawing is at 100%. */
export const K100 = PX_PER_MM / SCALE;
/** The most and least a person can zoom to, in percent. */
export const MIN_PERCENT = 10;
export const MAX_PERCENT = 2400;

export const kOfPercent = (percent: number) => (percent / 100) * K100;
export const percentOfK = (k: number) => (k / K100) * 100;
/** How a zoom reads: `146%`. */
export const percentLabel = (k: number) => `${Math.round(percentOfK(k))}%`;
/** The height in screen pixels of the reference and value text at zoom `k`. */
export const textPx = (k: number) => TEXT_MM * SCALE * k;
/** The zoom at which the text is `px` high. */
export const kForText = (px: number) => px / (TEXT_MM * SCALE);
export const MIN_LEGIBLE_K = kForText(MIN_TEXT_PX);
export const MAX_COMFORT_K = kForText(MAX_TEXT_PX);
/** The most a view is zoomed to frame a net or a part: text 11 px high, so what is round it stays in view. */
export const FRAME_MAX_K = kForText(11);

/** A box of the canvas, in canvas pixels. */
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** What the first view of a sheet is, and why. */
export interface Placement {
  view: ViewState;
  /** `fit` shows the whole box; `legible` is zoomed in so the text can be read; `comfort` is
   * held back so a small sheet is not enlarged past a sensible size. */
  mode: 'fit' | 'legible' | 'comfort';
  /** The zoom that shows the whole box. */
  fitK: number;
}

/** The view a sheet opens at in a window of `width` by `height` pixels. Where fitting the sheet
 * would make its text smaller than `MIN_TEXT_PX`, the zoom is the smallest that reads, with the
 * top left of the box at the top left of the window. A small sheet is shown no bigger than
 * `MAX_TEXT_PX` text, in the middle. */
export function placeSheet(width: number, height: number, box: Box, pad = 16): Placement {
  const fit = fitView(width, height, box);
  if (fit.k < MIN_LEGIBLE_K) {
    const k = MIN_LEGIBLE_K;
    return { view: { k, x: pad - box.x * k, y: pad - box.y * k }, mode: 'legible', fitK: fit.k };
  }
  if (fit.k > MAX_COMFORT_K) {
    const k = MAX_COMFORT_K;
    return {
      view: { k, x: (width - box.w * k) / 2 - box.x * k, y: (height - box.h * k) / 2 - box.y * k },
      mode: 'comfort',
      fitK: fit.k,
    };
  }
  return { view: fit, mode: 'fit', fitK: fit.k };
}

/** The view that makes the box as wide as the window, its top at the top. */
export function fitWidthView(width: number, box: Box, margin = 0.97, pad = 16): ViewState {
  const k = (width / box.w) * margin;
  return { k, x: (width - box.w * k) / 2 - box.x * k, y: pad - box.y * k };
}

/** The view that has the zoom `k` with the point `(px, py)` of the window on the same point of the canvas. */
export function zoomAbout(view: ViewState, k: number, px: number, py: number): ViewState {
  const f = k / view.k;
  return { k, x: px - (px - view.x) * f, y: py - (py - view.y) * f };
}

/** The range of zoom a person can reach, given the zoom that fits the sheet: down to half of
 * that, and never below `MIN_PERCENT`, up to `MAX_PERCENT`. */
export function zoomRange(fitK: number): { min: number; max: number } {
  return { min: Math.min(fitK * 0.5, kOfPercent(MIN_PERCENT)), max: kOfPercent(MAX_PERCENT) };
}

/** Whether the view is the "zoomed in to keep the text readable" one: the whole sheet would
 * have text below the smallest that reads, and the zoom is that smallest. */
export function isHeldForText(fitK: number, k: number): boolean {
  return fitK < MIN_LEGIBLE_K * 0.995 && Math.abs(k / MIN_LEGIBLE_K - 1) < 0.02;
}

/** The zoom that frames `box` with room around it: no smaller than `minK`, and no larger than a
 * small box gets by itself (text `FRAME_MAX_K` high, or `minK` if that is larger). This is what
 * "zoom to selection" uses. */
export function frameK(width: number, height: number, box: Box, minK = 0): number {
  const fit = Math.min(width / (box.w || 1), height / (box.h || 1)) * 0.7;
  return Math.min(Math.max(fit, minK), Math.max(FRAME_MAX_K, minK));
}

/** The view that puts the middle of `box` in the middle of the window. The zoom stays, unless it is
 * below `minK`; with `fitBox` it is the one that shows the box with room around it (`frameK`). */
export function centreOnBox(
  width: number,
  height: number,
  box: Box,
  current: ViewState,
  opts: { minK?: number; fitBox?: boolean } = {},
): ViewState {
  const k = opts.fitBox ? frameK(width, height, box, opts.minK ?? 0) : Math.max(current.k, opts.minK ?? 0);
  return { k, x: width / 2 - (box.x + box.w / 2) * k, y: height / 2 - (box.y + box.h / 2) * k };
}

/** The most a view is zoomed to bring a part into view after it was picked elsewhere, in percent. */
export const REVEAL_MAX_PERCENT = 200;
/** The zoom a view is raised to when the picked part is shown at a zoom too small to read (text 9 px). */
export const REVEAL_READ_K = kForText(9);

/** Whether `box` is inside the window with `margin` pixels to spare. */
export function inWindow(width: number, height: number, box: Box, view: ViewState, margin = 24): boolean {
  return (
    box.x * view.k + view.x >= margin &&
    (box.x + box.w) * view.k + view.x <= width - margin &&
    box.y * view.k + view.y >= margin &&
    (box.y + box.h) * view.k + view.y <= height - margin
  );
}

/** Where the view goes to show `part`, a part someone picked in another view (go to anything, the
 * problems list, the PCB), without losing the place they were looking at.
 *
 * The view stays as it is when the text is readable (`minK`) and the part is on screen. Otherwise
 * the part is put in the middle, at the zoom the view has, raised to a readable one if it is
 * below, and never above `REVEAL_MAX_PERCENT` or above what "zoom to selection" gives for
 * `context` (the part and its neighbours), so what is round the part stays in view.
 * `kept` says the view did not move. */
export function revealPart(
  width: number,
  height: number,
  part: Box,
  context: Box,
  current: ViewState,
  minK = MIN_LEGIBLE_K,
): { view: ViewState; kept: boolean } {
  if (current.k >= minK && inWindow(width, height, part, current)) return { view: current, kept: true };
  const cap = Math.min(kOfPercent(REVEAL_MAX_PERCENT), frameK(width, height, context, minK));
  const k = current.k < minK ? Math.min(cap, Math.max(minK, REVEAL_READ_K)) : Math.min(current.k, cap);
  return {
    view: { k, x: width / 2 - (part.x + part.w / 2) * k, y: height / 2 - (part.y + part.h / 2) * k },
    kept: false,
  };
}

/** The view that moves no more than it must to bring `box` into the window with `margin` pixels
 * around it. Where the window is below `minK` it is raised to it and the box is centred. */
export function revealBox(
  width: number,
  height: number,
  box: Box,
  current: ViewState,
  minK = 0,
  margin = 40,
): ViewState {
  if (current.k < minK) return centreOnBox(width, height, box, current, { minK });
  const shift = (from: number, size: number, at: number, room: number) => {
    const lo = from * current.k + at;
    const hi = (from + size) * current.k + at;
    if (hi - lo + 2 * margin > room) return room / 2 - ((from + size / 2) * current.k + at);
    if (lo < margin) return margin - lo;
    if (hi > room - margin) return room - margin - hi;
    return 0;
  };
  return {
    k: current.k,
    x: current.x + shift(box.x, box.w, current.x, width),
    y: current.y + shift(box.y, box.h, current.y, height),
  };
}
