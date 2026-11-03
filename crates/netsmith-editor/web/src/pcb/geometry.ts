// Plain geometry for the PCB canvas: millimetres, y grows downward, angles are degrees counter-
// clockwise on screen, as in the layout file. No DOM in here, so it is tested on its own.

import type { PcbBox } from '../api';

export interface Pt {
  x: number;
  y: number;
}

/** `v` on the grid (a grid of 0 is off). */
export function snap(v: number, grid: number): number {
  return grid > 0 ? Math.round(v / grid) * grid : v;
}

/** The layout file keeps hundredths of a millimetre; this drops the float dust of a sum. */
export function tidy(v: number): number {
  return Math.round(v * 100) / 100;
}

/** How far to move something at `anchor` so that it ends on the grid when the pointer has moved
 * by `by`: the whole selection moves by the same amount, so it keeps its shape. */
export function snapMove(anchor: Pt, by: Pt, grid: number): Pt {
  return { x: snap(anchor.x + by.x, grid) - anchor.x, y: snap(anchor.y + by.y, grid) - anchor.y };
}

/** An angle in 0..360. */
export function normAngle(degrees: number): number {
  return ((degrees % 360) + 360) % 360;
}

/** `p` turned about `c` by `degrees`, counter-clockwise on screen (y is down). */
export function rotateAbout(p: Pt, c: Pt, degrees: number): Pt {
  const r = (degrees * Math.PI) / 180;
  const [cos, sin] = [Math.cos(r), Math.sin(r)];
  const [dx, dy] = [p.x - c.x, p.y - c.y];
  return { x: c.x + dx * cos + dy * sin, y: c.y - dx * sin + dy * cos };
}

export function boxFrom(a: Pt, b: Pt): PcbBox {
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), width: Math.abs(a.x - b.x), height: Math.abs(a.y - b.y) };
}

export function contains(b: PcbBox, p: Pt): boolean {
  return p.x >= b.x && p.x <= b.x + b.width && p.y >= b.y && p.y <= b.y + b.height;
}

export function intersects(a: PcbBox, b: PcbBox): boolean {
  return a.x <= b.x + b.width && b.x <= a.x + a.width && a.y <= b.y + b.height && b.y <= a.y + a.height;
}

export function unite(boxes: PcbBox[]): PcbBox | null {
  if (boxes.length === 0) return null;
  const x0 = Math.min(...boxes.map((b) => b.x));
  const y0 = Math.min(...boxes.map((b) => b.y));
  const x1 = Math.max(...boxes.map((b) => b.x + b.width));
  const y1 = Math.max(...boxes.map((b) => b.y + b.height));
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

export function centre(b: PcbBox): Pt {
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

export function moved(b: PcbBox, by: Pt): PcbBox {
  return { ...b, x: b.x + by.x, y: b.y + by.y };
}

/** The part under the point: of the courtyards that hold it, the smallest, so a small part lying
 * on a big one can still be picked. */
export function hitBox<T extends { courtyard: PcbBox }>(things: T[], p: Pt): T | null {
  let best: T | null = null;
  let area = Infinity;
  for (const t of things) {
    const a = t.courtyard.width * t.courtyard.height;
    if (contains(t.courtyard, p) && a <= area) {
      best = t;
      area = a;
    }
  }
  return best;
}

/** What a box selection takes: whatever's courtyard touches the box. */
export function inBox<T extends { courtyard: PcbBox }>(things: T[], box: PcbBox): T[] {
  return things.filter((t) => intersects(t.courtyard, box));
}

export type Handle = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';

/** The eight handles of a box to resize it by, where they are. */
export function handlesOf(b: PcbBox): { handle: Handle; x: number; y: number }[] {
  const [l, m, r] = [b.x, b.x + b.width / 2, b.x + b.width];
  const [t, c, d] = [b.y, b.y + b.height / 2, b.y + b.height];
  return [
    { handle: 'nw', x: l, y: t },
    { handle: 'n', x: m, y: t },
    { handle: 'ne', x: r, y: t },
    { handle: 'e', x: r, y: c },
    { handle: 'se', x: r, y: d },
    { handle: 's', x: m, y: d },
    { handle: 'sw', x: l, y: d },
    { handle: 'w', x: l, y: c },
  ];
}

/** The handle within `reach` of the point, if there is one. */
export function hitHandle(b: PcbBox, p: Pt, reach: number): Handle | null {
  let best: Handle | null = null;
  let nearest = reach;
  for (const h of handlesOf(b)) {
    const d = Math.hypot(h.x - p.x, h.y - p.y);
    if (d <= nearest) {
      best = h.handle;
      nearest = d;
    }
  }
  return best;
}

/** The box with the edges its handle holds moved by `by`, and no smaller than `least` in either
 * direction (the opposite edge stays where it is). */
export function resizeBox(b: PcbBox, handle: Handle, by: Pt, least = 0.5): PcbBox {
  let [x0, y0, x1, y1] = [b.x, b.y, b.x + b.width, b.y + b.height];
  if (handle.includes('w')) x0 = Math.min(x0 + by.x, x1 - least);
  if (handle.includes('e')) x1 = Math.max(x1 + by.x, x0 + least);
  if (handle.includes('n')) y0 = Math.min(y0 + by.y, y1 - least);
  if (handle.includes('s')) y1 = Math.max(y1 + by.y, y0 + least);
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** The SVG transform of a part's group: the same as `netsmith_pcb::svg` writes (`translate`,
 * then `rotate` clockwise-positive so the part turns counter-clockwise, then a mirror on the
 * back). */
export function partTransform(x: number, y: number, rotation: number, bottom: boolean): string {
  let t = `translate(${x} ${y})`;
  if (rotation !== 0) t += ` rotate(${-rotation})`;
  if (bottom) t += ' scale(-1 1)';
  return t;
}
