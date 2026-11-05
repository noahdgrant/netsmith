// The sizes of the system view's cards. The page's styles use the same numbers for the rows, so
// the height of a card here is the height it has there.

import type { ModuleModel } from './model';
import { SHOWN } from './model';

export const CARD_W = 304;
const PAD_TOP = 8;
const PAD_BOTTOM = 8;
export const HEAD = 28;
export const ROW = 20;
/** The "N ports" line that opens the table. */
export const FOLD = 26;
/** Where edges meet a card: the middle of its head. */
export const ANCHOR = PAD_TOP + HEAD / 2;
/** The space between the ends of the edges on a card: room for a label on each. */
export const STEP = 24;

/** Whether the table of ports is shown in full: few ports are always, many when opened. */
export const showsAll = (m: ModuleModel, open: boolean): boolean => m.ports.length <= SHOWN || open;

export function cardHeight(m: ModuleModel, open: boolean, sides: { in: number; out: number }): number {
  let h = PAD_TOP + HEAD + PAD_BOTTOM;
  if (m.ports.length > 0) {
    h += m.ports.length <= SHOWN ? m.ports.length * ROW + 2 : FOLD + (open ? m.ports.length * ROW : 0);
  }
  // Room for the ends of the edges, three grid steps apart.
  const ends = Math.max(sides.in, sides.out);
  return Math.max(h, PAD_TOP + 8 + (Math.max(ends, 1) + 1) * STEP);
}

/** How wide a label is in the page's font, for the room the layout keeps for it. */
export const labelWidth = (text: string): number => Math.ceil(text.length * 6.8) + 8;
