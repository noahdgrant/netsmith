import { describe, expect, it } from 'vitest';

import type { SchematicHints } from '../api';
import { countIn, sheetAt, tablesOf, toToml, withSheetAt, withoutSheet } from './hints';
import { type PlaceCtx, clearPlaceEdit, cornerOf, isCut, placeEdit, placeOf, placeable } from './sheetplace';
import { SCALE } from './zoom';

const data = (boards: SchematicHints['boards'], errors: SchematicHints['errors'] = []): SchematicHints => ({
  board: 'Big',
  file: 'src/main.sch.toml',
  exists: true,
  stamp: 's',
  text: '',
  errors,
  boards,
  findings: [],
  version: 1n,
});

const modules = new Set(['supply1', 'supply2', 'stage']);
const ctx = (boards: SchematicHints['boards'] = {}, errors: SchematicHints['errors'] = []): PlaceCtx => ({ board: 'Big', definition: 'Big', data: data(boards, errors), modules });
const corners = new Map<string, [number, number]>([
  ['supply1', [0, 0]],
  ['supply2', [101.6, 0]],
  ['Big', [0, 76.2]],
  ['stage (1/2)', [101.6, 76.2]],
]);

describe('which cards can be dragged', () => {
  it('a module sheet and the board own sheet can, a sheet cut into boxes cannot', () => {
    expect(isCut('stage (2/3)')).toBe(true);
    expect(isCut('supply1')).toBe(false);
    expect(placeable('supply1', modules)).toBe('supply1');
    expect(placeable('Big', modules)).toBe(null);
    expect(placeable('stage (1/2)', modules)).toBe(undefined);
  });
  it('reads a corner of the drawing on the grid, from the page origin', () => {
    const sheet = { name: 'supply2', title: '', x: 100 * SCALE, y: 2 * SCALE, w: 10, h: 10 };
    expect(cornerOf(sheet, { x: 1.7, y: -2 })).toEqual([101.6, 0]);
  });
});

describe('a drop', () => {
  it('puts the dragged sheet at the corner of the card it is dropped on', () => {
    const edit = placeEdit(ctx(), 'supply2', 'supply1', corners)!;
    const after = edit.apply({})!;
    expect(sheetAt(after, 'Big', 'supply2')).toEqual([0, 0]);
    expect(toToml(after)).toBe('version = 1\n\n[Big.sheets.supply2]\nat = [0, 0]\n');
    expect(edit.entry).toEqual({ kind: 'sheet', places: [{ sheet: 'supply2', before: undefined }] });
  });
  it('swaps with a card that has a place of its own', () => {
    const c = ctx({ Big: { parts: {}, notes: {}, sheets: { supply1: { at: { x: 0, y: 0, line: 3 }, parts: {}, notes: {} } } } });
    expect(placeOf(c, 'supply1')).toEqual([0, 0]);
    const edit = placeEdit(c, 'Big', 'supply1', corners)!;
    const after = edit.apply(tablesOf(c.data.boards))!;
    expect(sheetAt(after, 'Big', null)).toEqual([0, 0]);
    expect(sheetAt(after, 'Big', 'supply1')).toEqual([0, 76.2]);
    expect(toToml(after)).toBe('version = 1\n\n[Big]\nat = [0, 0]\n\n[Big.sheets.supply1]\nat = [0, 76.2]\n');
  });
  it('writes nothing on its own card, for a cut sheet, or into a file with a fault', () => {
    expect(placeEdit(ctx(), 'supply1', 'supply1', corners)).toBeNull();
    expect(placeEdit(ctx(), 'stage (1/2)', 'supply1', corners)).toBeNull();
    expect(placeEdit(ctx({}, [{ line: 2, message: 'bad' }]), 'supply1', 'supply2', corners)).toBeNull();
  });
  it('a place is taken out again, and goes with the module when its layout is cleared', () => {
    const c = ctx({ Big: { parts: {}, notes: {}, sheets: { supply1: { at: { x: 0, y: 0, line: 3 }, parts: {}, notes: {} } } } });
    expect(clearPlaceEdit(c, 'supply1')!.apply(tablesOf(c.data.boards))).toEqual({});
    expect(clearPlaceEdit(c, 'supply2')).toBeNull();
    const t = tablesOf(c.data.boards);
    expect(countIn(t, 'Big', 'supply1')).toBe(1);
    expect(withoutSheet(t, 'Big', 'supply1')).toEqual({});
    expect(withoutSheet(withSheetAt({}, 'Big', null, [0, 0]), 'Big', null)).toEqual({});
  });
});
