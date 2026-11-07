import { describe, expect, it } from 'vitest';

import type { HintBoard, SchematicHints } from '../api';
import { type NoteFacts, type PartFacts, hintSheet, moduleNames, noteKeyOf, pinFrame, readFacts, readNotes } from './blocks';
import { type Ctx, clearAllEdit, clearPartEdit, clearSheetEdit, dragRefusal, dropEdit, hasEntry, mirrorEdit, noteDropEdit, nudgeEdit, turnEdit, writable } from './moves';
import { type Tables, partEntry, tablesOf } from './hints';
import { refusalAt } from '../views/schematic-layout';

const facts = (...fs: Partial<PartFacts>[]): Map<string, PartFacts> =>
  new Map(
    fs.map((f) => {
      const full: PartFacts = {
        ref: 'R1',
        path: 'r1',
        address: { sheet: null, key: 'r1' },
        orient: { mirror: false, turns: 0 },
        origin: [0, 0],
        anchor: false,
        slot: 'Led',
        ...f,
      };
      return [full.ref, full] as const;
    }),
  );

const hints = (boards: Record<string, HintBoard> = {}, over: Partial<SchematicHints> = {}): SchematicHints => ({
  board: 'x/led',
  file: 'src/main.sch.toml',
  exists: true,
  stamp: '1',
  text: '',
  errors: [],
  boards,
  findings: [],
  version: 1n,
  ...over,
});

const ctx = (data: SchematicHints, fs: Map<string, PartFacts>): Ctx => ({ board: 'x/led', definition: 'Led', data, facts: fs, modules: new Set(['stage']) });
const apply = (c: Ctx, edit: ReturnType<typeof dropEdit>): Tables => edit!.apply(tablesOf(c.data.boards))!;
const PARTS = facts(
  { ref: 'U1', path: 'u1', address: { sheet: null, key: 'u1' }, anchor: true, origin: [0, 0] },
  { ref: 'R1', path: 'r1', address: { sheet: null, key: 'r1' }, origin: [10.16, 2.54] },
  { ref: 'C1', path: 'stage.c1', address: { sheet: 'stage', key: 'c1' }, orient: { mirror: true, turns: 1 }, slot: 'stage' },
);

describe('a drop', () => {
  it('writes an offset for the part, in its sheet, from nothing', () => {
    const c = ctx(hints(), PARTS);
    expect(apply(c, dropEdit(c, 'R1', 2, -1, false))).toEqual({ Led: { parts: { r1: { dx: 2.54, dy: -1.27 } }, notes: {}, sheets: {} } });
  });
  it('writes it in the table of the module the part is in', () => {
    const c = ctx(hints(), PARTS);
    const t = apply(c, dropEdit(c, 'C1', 1, 0, false));
    expect(partEntry(t, 'Led', { sheet: 'stage', key: 'c1' })).toEqual({ dx: 1.27, dy: 0 });
  });
  it('adds to the offset the part has and keeps the other boards of the file', () => {
    const c = ctx(hints({ Led: { parts: { r1: { dx: 1.27, dy: 0, line: 3 } }, notes: {}, sheets: {} }, Other: { parts: { x: { dx: 5.08, dy: 0, line: 9 } }, notes: {}, sheets: {} } }), PARTS);
    const t = apply(c, dropEdit(c, 'R1', 1, 1, false));
    expect(partEntry(t, 'Led', { sheet: null, key: 'r1' })).toEqual({ dx: 2.54, dy: 1.27 });
    expect(partEntry(t, 'Other', { sheet: null, key: 'x' })).toEqual({ dx: 5.08, dy: 0 });
  });
  it('starts from the engine’s place when the drawing refused the entry', () => {
    const c = ctx(
      hints(
        { Led: { parts: { r1: { dx: 25.4, dy: 0, line: 3 } }, notes: {}, sheets: {} } },
        { findings: [{ kind: 'unusable', line: 3, entry: '[Led.parts.r1]', message: '[Led.parts.r1]: its block would lie on the part `u1` at the place the file gives it; it is ignored' }] },
      ),
      PARTS,
    );
    expect(partEntry(apply(c, dropEdit(c, 'R1', 1, 0, false)), 'Led', { sheet: null, key: 'r1' })).toEqual({ dx: 1.27, dy: 0 });
  });
  it('pins the part from the sheet’s reference part with the modifier', () => {
    const c = ctx(hints(), PARTS);
    // R1's origin is 8 steps right and 2 down of the reference part's; dropped 2 further right.
    expect(partEntry(apply(c, dropEdit(c, 'R1', 2, 0, true)), 'Led', { sheet: null, key: 'r1' })).toEqual({ pin: [12.7, 2.54] });
  });
  it('measures a pin from where the engine put the reference part, not where an entry moved it', () => {
    const c = ctx(hints({ Led: { parts: { u1: { dx: 2.54, dy: 0, line: 3 } }, notes: {}, sheets: {} } }), facts(
      { ref: 'U1', path: 'u1', address: { sheet: null, key: 'u1' }, anchor: true, origin: [2.54, 0] },
      { ref: 'R1', path: 'r1', address: { sheet: null, key: 'r1' }, origin: [10.16, 2.54] },
    ));
    expect(partEntry(apply(c, dropEdit(c, 'R1', 0, 0, true)), 'Led', { sheet: null, key: 'r1' })).toEqual({ pin: [10.16, 2.54] });
  });
  it('does not move the reference part: it is the sheet’s anchor', () => {
    const c = ctx(hints(), PARTS);
    expect(dragRefusal(c, 'U1')).toMatch(/anchor: drag a neighbour/);
    expect(dropEdit(c, 'U1', 1, 0, false)).toBeNull();
    expect(nudgeEdit(c, 'U1', 1, 0)).toBeNull();
    expect(dragRefusal(c, 'R1')).toBeNull();
  });
  it('writes nothing into a file with a fault: it is ignored as a whole and writing would lose it', () => {
    const c = ctx(hints({}, { errors: [{ line: 4, message: 'unknown key' }] }), PARTS);
    expect(writable(c.data)).toBe(false);
    expect(dragRefusal(c, 'R1')).toMatch(/error/);
    expect(dropEdit(c, 'R1', 1, 0, false)).toBeNull();
    expect(turnEdit(c, 'R1', 1)).toBeNull();
  });
  it('removes the entry when the move comes back to the start', () => {
    const c = ctx(hints({ Led: { parts: { r1: { dx: 1.27, dy: 0, line: 3 } }, notes: {}, sheets: {} } }), PARTS);
    expect(apply(c, dropEdit(c, 'R1', -1, 0, false))).toEqual({});
  });
});

describe('the keys', () => {
  it('nudge by the steps given, from what the part has', () => {
    const c = ctx(hints({ Led: { parts: { r1: { dx: 1.27, dy: 0, line: 3 } }, notes: {}, sheets: {} } }), PARTS);
    expect(partEntry(apply(c, nudgeEdit(c, 'R1', 10, 0)), 'Led', { sheet: null, key: 'r1' })).toEqual({ dx: 13.97, dy: 0 });
    expect(partEntry(apply(c, nudgeEdit(c, 'R1', 0, -1)), 'Led', { sheet: null, key: 'r1' })).toEqual({ dx: 1.27, dy: -1.27 });
  });
  it('turn from the orientation the drawing has, and the anchor can be turned', () => {
    const c = ctx(hints(), PARTS);
    expect(partEntry(apply(c, turnEdit(c, 'C1', 1)), 'Led', { sheet: 'stage', key: 'c1' })).toEqual({ rotate: 180 });
    expect(partEntry(apply(c, turnEdit(c, 'U1', 1)), 'Led', { sheet: null, key: 'u1' })).toEqual({ rotate: 90 });
  });
  it('mirror from the mirror the drawing has, and keep an offset', () => {
    const c = ctx(hints({ Led: { parts: { r1: { dx: 1.27, dy: 0, line: 3 } }, notes: {}, sheets: {} } }), PARTS);
    expect(partEntry(apply(c, mirrorEdit(c, 'R1')), 'Led', { sheet: null, key: 'r1' })).toEqual({ dx: 1.27, dy: 0, mirror: true });
    expect(partEntry(apply(c, mirrorEdit(c, 'C1')), 'Led', { sheet: 'stage', key: 'c1' })).toEqual({ mirror: false });
  });
  it('clear the entry of a part that has one, and only then', () => {
    const c = ctx(hints({ Led: { parts: { r1: { dx: 1.27, dy: 0, line: 3 } }, notes: {}, sheets: {} } }), PARTS);
    expect(hasEntry(c, 'R1')).toBe(true);
    expect(hasEntry(c, 'C1')).toBe(false);
    expect(apply(c, clearPartEdit(c, 'R1'))).toEqual({});
    expect(clearPartEdit(c, 'C1')).toBeNull();
  });
});

describe('clearing', () => {
  const data = hints({
    Led: { parts: { r1: { dx: 1.27, dy: 0, line: 3 } }, notes: {}, sheets: { stage: { parts: { c1: { rotate: 90, line: 6 } }, notes: {} } } },
    Other: { parts: { x: { dx: 5.08, dy: 0, line: 9 } }, notes: {}, sheets: {} },
  });
  const c = ctx(data, PARTS);
  it('clears one sheet and keeps the rest of the file', () => {
    const t = clearSheetEdit(c, 'stage')!.apply(tablesOf(data.boards))!;
    expect(t.Led.sheets).toEqual({});
    expect(t.Led.parts.r1).toEqual({ dx: 1.27, dy: 0 });
    expect(t.Other).toBeDefined();
  });
  it('clears the file for every board', () => {
    expect(clearAllEdit(c)!.apply(tablesOf(data.boards))).toEqual({});
  });
  it('has nothing to do for a sheet or a file without entries', () => {
    expect(clearSheetEdit(ctx(hints(), PARTS), 'stage')).toBeNull();
    expect(clearAllEdit(ctx(hints(), PARTS))).toBeNull();
  });
});

describe('a note', () => {
  const note: NoteFacts = { index: 2, path: 'stage.c1', kind: 'doc', nth: 1, slot: 'stage (1/2)' };
  it('is moved by an offset in its sheet’s table, keyed by its path from the sheet', () => {
    const c = ctx(hints(), PARTS);
    const t = noteDropEdit(c, note, 4, 0)!.apply({})!;
    expect(t.Led.sheets.stage.notes['c1#doc']).toEqual({ dx: 5.08, dy: 0 });
  });
  it('adds to the offset it has', () => {
    const c = ctx(hints({ Led: { parts: {}, notes: {}, sheets: { stage: { parts: {}, notes: { 'c1#doc': { dx: 5.08, dy: 0, line: 4 } } } } } }), PARTS);
    const t = noteDropEdit(c, note, 0, 2)!.apply(tablesOf(c.data.boards))!;
    expect(t.Led.sheets.stage.notes['c1#doc']).toEqual({ dx: 5.08, dy: 2.54 });
  });
});

describe('what the drawing says of its parts and notes', () => {
  const el = (attrs: Record<string, string>, extra: { slot?: string; classes?: string[] } = {}) => ({
    getAttribute: (n: string) => attrs[n] ?? null,
    hasAttribute: (n: string) => n in attrs,
    classList: { contains: (c: string) => (extra.classes ?? []).includes(c) },
    closest: () => ({ getAttribute: () => extra.slot ?? '' }),
  });
  it('reads the origin, the turn and the anchor of each part, and the first box of a part with units is its own', () => {
    const root = {
      querySelectorAll: () => [
        el({ 'data-ref': 'U1', 'data-origin': '10 -5.08', 'data-orient': '1 2', 'data-anchor': '1' }, { slot: 'stage' }),
        el({ 'data-ref': 'U1', 'data-origin': '30 0', 'data-orient': '0 0' }, { slot: 'stage' }),
        el({ 'data-ref': 'R1', 'data-origin': '1 2', 'data-orient': '0 0' }, { slot: 'stage' }),
        el({ 'data-ref': 'X9', 'data-origin': '1 2' }),
      ],
    } as unknown as ParentNode;
    const f = readFacts(root, (r) => ({ U1: 'stage.u1', R1: 'stage.r1' })[r]);
    expect([...f.keys()]).toEqual(['U1', 'R1']);
    expect(f.get('U1')).toMatchObject({ path: 'stage.u1', address: { sheet: 'stage', key: 'u1' }, origin: [10, -5.08], orient: { mirror: true, turns: 2 }, anchor: true, slot: 'stage' });
    expect(f.get('R1')!.anchor).toBe(false);
  });
  it('finds the reference part of a part’s sheet, and a cut sheet has one for each box', () => {
    const f = facts(
      { ref: 'A', address: { sheet: 'stage', key: 'a' }, slot: 'stage (1/2)', anchor: true },
      { ref: 'B', address: { sheet: 'stage', key: 'b' }, slot: 'stage (1/2)' },
      { ref: 'C', address: { sheet: 'stage', key: 'c' }, slot: 'stage (2/2)', anchor: true, origin: [50, 0] },
      { ref: 'D', address: { sheet: 'stage', key: 'd' }, slot: 'stage (2/2)', origin: [60, 0] },
    );
    expect(pinFrame(f, f.get('D')!, () => ({ entry: undefined, honoured: true }))!.frame).toEqual([50, 0]);
    expect(pinFrame(f, f.get('B')!, () => ({ entry: undefined, honoured: true }))!.frame).toEqual([0, 0]);
  });
  it('names the sheet of a drawn sheet: the cut number goes, the board’s own box is no module', () => {
    const modules = moduleNames(['j1', 'stage.c1', 'stage.filter.l1', 'radio.u1']);
    expect([...modules].sort()).toEqual(['radio', 'stage']);
    expect(hintSheet('stage (2/3)', modules)).toBe('stage');
    expect(hintSheet('radio', modules)).toBe('radio');
    expect(hintSheet('Led', modules)).toBeNull();
    expect(hintSheet('', modules)).toBeNull();
  });
  it('counts the notes of one path and kind in the design’s order, as the file does', () => {
    const rows = [
      { 'data-n': '5', 'data-note': 'stage' },
      { 'data-n': '2', 'data-note': 'stage' },
      { 'data-n': '3', 'data-note': 'stage.c1' },
    ];
    const root = {
      querySelectorAll: () => rows.map((r) => el(r, { slot: 'stage', classes: r['data-note'] === 'stage' ? ['note-relation'] : ['note-doc'] })),
    } as unknown as ParentNode;
    const notes = readNotes(root);
    expect(noteKeyOf(notes.get(2)!, 'stage')).toBe('#relation');
    expect(noteKeyOf(notes.get(5)!, 'stage')).toBe('#relation2');
    expect(noteKeyOf(notes.get(3)!, 'stage')).toBe('c1#doc');
  });
});

describe('a drop the drawing refuses', () => {
  const finding = { kind: 'unusable' as const, line: 7, entry: '[Led.parts.r1]', message: '[Led.parts.r1]: its block would lie on the part `u1` at the place the file gives it; it is ignored' };
  it('is named by its reason, without the entry and without what happens to it', () => {
    expect(refusalAt(hints({}, { findings: [finding] }), 7)).toBe('its block would lie on the part `u1` at the place the file gives it');
  });
  it('is not a refusal when the entry is not the one refused, or has no line, or the finding is another kind', () => {
    expect(refusalAt(hints({}, { findings: [finding] }), 8)).toBeNull();
    expect(refusalAt(hints({}, { findings: [finding] }), null)).toBeNull();
    expect(refusalAt(hints({}, { findings: [{ ...finding, kind: 'stale_entry' }] }), 7)).toBeNull();
  });
});
