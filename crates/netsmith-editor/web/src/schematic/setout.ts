// The order a board drawn by module sets its sheets out in: row by row, each row from the left.
// The drawing (crates/netsmith-view/src/sheets.rs, `connection_order`) puts the sheets most tied by
// nets one after the other, so the overview's cards in this order have connected modules side by
// side. In a row the tops differ by up to a grid step (each sheet is snapped to the grid on its
// own), while rows are a sheet's height and a gap apart, so a row is the sheets whose tops are
// within `ROW` of each other.

import type { Sheet } from './drawing';
import { SCALE } from './zoom';

/** How far apart, in the drawing's units (`SCALE` to the millimetre), the tops of two sheets of one row may be: more than a grid step (1.27 mm), less than the gap between rows (12.7 mm). */
export const ROW = 5 * SCALE;

/** `items` in the order the drawing sets their sheets out: by row, then from the left. */
export function setOut<T>(items: readonly T[], sheet: (item: T) => Sheet): T[] {
  return [...items].sort((a, b) => {
    const [p, q] = [sheet(a), sheet(b)];
    return Math.abs(p.y - q.y) < ROW ? p.x - q.x : p.y - q.y;
  });
}
