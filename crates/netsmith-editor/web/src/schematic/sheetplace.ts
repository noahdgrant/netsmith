// Where a sheet goes on the page, as the overview's drag writes it (ADR 0139): `at = [x, y]` in the sheet's table of the layout
// file, the top-left corner of its box in millimetres from the page's origin. A drop on a card puts the dragged sheet at the
// corner that card's sheet has in the drawing, so the dragged card takes its place and the sheets never moved are set out
// again round it by their connections. When the sheet dropped on has a place of its own the two swap. Pure functions: the
// overview reads the corners from the drawing and hands the edit to the one writer (`views/schematic-layout.ts`).

import type { SchematicHints } from '../api';
import type { Edit } from '../views/schematic-layout';
import { hintSheet } from './blocks';
import { mmToSteps, sheetAt, stepsToMm, tablesOf, withSheetAt } from './hints';
import type { Sheet } from './drawing';
import { SCALE } from './zoom';

/** A sheet cut into numbered boxes (`stage (2/3)`) has no one place, so its card cannot be dragged. */
export const isCut = (name: string): boolean => / \(\d+\/\d+\)$/.test(name);

/** The top-left corner of a sheet's box from the page's origin, in millimetres on the grid (`mm` is where the drawing's frame
 * starts, `Drawing.mm`). */
export function cornerOf(sheet: Sheet, mm: { x: number; y: number }): [number, number] {
  return [stepsToMm(mmToSteps(sheet.x / SCALE + mm.x)), stepsToMm(mmToSteps(sheet.y / SCALE + mm.y))];
}

/** What a sheet is to the file: its table (`null` for the board's own sheet) when its card can be dragged, else `undefined`. */
export function placeable(name: string, modules: ReadonlySet<string>): string | null | undefined {
  return isCut(name) ? undefined : hintSheet(name, modules);
}

export interface PlaceCtx {
  board: string;
  definition: string;
  data: SchematicHints;
  modules: ReadonlySet<string>;
}

/** The place the file gives the sheet of a card, if any. */
export function placeOf(ctx: PlaceCtx, name: string): [number, number] | undefined {
  const sheet = placeable(name, ctx.modules);
  return sheet === undefined ? undefined : sheetAt(tablesOf(ctx.data.boards), ctx.definition, sheet);
}

/** The edit a drop of the card `from` on the card `to` makes, or null when it makes none: the same card, a card that cannot be
 * dragged, a file with a fault (the drawing ignores it, so writing would drop it). `corners` are the corners of the sheets as
 * drawn, by name. */
export function placeEdit(ctx: PlaceCtx, from: string, to: string, corners: ReadonlyMap<string, [number, number]>): Edit | null {
  const sheet = placeable(from, ctx.modules);
  const at = corners.get(to);
  const back = corners.get(from);
  if (from === to || sheet === undefined || !at || !back || ctx.data.errors.length > 0) return null;
  const tables = tablesOf(ctx.data.boards);
  const other = placeable(to, ctx.modules);
  const otherBefore = other === undefined ? undefined : sheetAt(tables, ctx.definition, other);
  // The sheet dropped on keeps a place of its own: it takes the dragged one's corner.
  const swap = other !== undefined && otherBefore !== undefined;
  return {
    board: ctx.board,
    definition: ctx.definition,
    label: `${from} placed`,
    apply: (t) => {
      let next = withSheetAt(t, ctx.definition, sheet, at);
      if (swap) next = withSheetAt(next, ctx.definition, other, back);
      return next;
    },
    entry: {
      kind: 'sheet',
      places: [{ sheet, before: sheetAt(tables, ctx.definition, sheet) }, ...(swap ? [{ sheet: other, before: otherBefore }] : [])],
    },
  };
}

/** The edit that takes the place of a card's sheet out of the file (it goes back to the arrangement), or null when it has none. */
export function clearPlaceEdit(ctx: PlaceCtx, name: string): Edit | null {
  const sheet = placeable(name, ctx.modules);
  if (sheet === undefined || ctx.data.errors.length > 0) return null;
  const before = sheetAt(tablesOf(ctx.data.boards), ctx.definition, sheet);
  if (!before) return null;
  return {
    board: ctx.board,
    definition: ctx.definition,
    label: `${name} put back`,
    apply: (t) => withSheetAt(t, ctx.definition, sheet, null),
    entry: { kind: 'sheet', places: [{ sheet, before }] },
  };
}
