// The sizes of what the power tree draws. The page's styles use the same numbers for the rows of
// a card, so the height of a card here is the height it has there.

import type { RailModel, StageModel } from './model';

export const RAIL_W = 264;
export const STAGE_W = 200;
/** The room above and below the content of a card. */
const PAD_TOP = 9;
const PAD_BOTTOM = 8;
/** Name row, voltage row. */
export const HEAD = 24;
export const VOLT = 20;
export const ROW = 20;
/** The line of the total, the meter, its note. */
const TOTAL = 6 + 22;
const METER = 8;
const NOTE = 20;
/** The "N loads" line. */
export const SUMMARY = 30;
/** "+N more" and "N loads at 0 A". */
export const FOLD = 22;
/** Supplies shown on a card before "+N more supplies". */
export const SUPPLIES = 3;

/** What of a rail card is open. */
export interface Open {
  /** The list of loads is shown. */
  list: boolean;
  /** Every load that draws something, not just the heaviest. */
  more: boolean;
  /** The loads at 0 A, one by one. */
  zeros: boolean;
}

export const closed: Open = { list: false, more: false, zeros: false };

/** The rows of the loads list, in order: what it shows of a rail. */
export function listRows(r: RailModel, open: Open): { loads: number; foldMore: number; foldZeros: number; unknown: number } {
  return {
    loads: Math.min(r.top.length + (open.more ? r.more.length : 0), r.top.length + r.more.length),
    foldMore: open.more || r.more.length === 0 ? 0 : r.more.length,
    foldZeros: r.zeros.length,
    unknown: r.unknowns.length,
  };
}

/** How many rows of supplies a card shows. */
export const supplyRows = (r: RailModel): number => Math.min(r.supplies.length, SUPPLIES) + (r.supplies.length > SUPPLIES ? 1 : 0);

export function railHeight(r: RailModel, open: Open): number {
  let h = PAD_TOP + HEAD + VOLT + (r.supplies.length ? 4 + supplyRows(r) * ROW : 0) + TOTAL + METER + NOTE + SUMMARY + PAD_BOTTOM;
  if (open.list) {
    const rows = listRows(r, open);
    h += rows.loads * ROW + rows.unknown * ROW + (rows.foldMore ? FOLD : 0) + (rows.foldZeros ? FOLD + (open.zeros ? rows.foldZeros * ROW : 0) : 0);
  }
  return h;
}

/** Where edges meet a card: the middle of its name row. */
export const RAIL_ANCHOR = PAD_TOP + HEAD / 2;

export function stageHeight(s: StageModel): number {
  return 60 + (s.efficiency || s.efficiencyUnknown ? ROW - 2 : 0);
}
