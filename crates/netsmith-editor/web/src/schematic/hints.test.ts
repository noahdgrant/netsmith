import { describe, expect, it } from 'vitest';

import type { HintBoard } from '../api';
import {
  GRID_MM,
  type Tables,
  addressOf,
  countAll,
  countIn,
  isBlank,
  keyFrom,
  keyText,
  mmToSteps,
  noteAddress,
  numberText,
  partEntry,
  stepsToMm,
  tablesOf,
  toToml,
  withNote,
  withPart,
  withoutSheet,
} from './hints';

const board: HintBoard = {
  parts: { j1: { dx: 2.54, dy: -1.27, line: 3 } },
  notes: {},
  sheets: {
    stage: {
      parts: { output_cap: { rotate: 90, mirror: true, line: 7 }, 'filter.l1': { pin: [12.7, 0], line: 11 } },
      notes: { 'output_cap#doc': { dx: 5.08, dy: 0, line: 15 } },
    },
  },
};

describe('the grid', () => {
  it('turns steps into millimetres with the digits the file spells, and back', () => {
    expect(GRID_MM).toBe(1.27);
    expect(stepsToMm(3)).toBe(3.81);
    expect(stepsToMm(-1)).toBe(-1.27);
    expect(stepsToMm(0)).toBe(0);
    expect(Object.is(stepsToMm(-0), 0)).toBe(true);
    expect(mmToSteps(3.81)).toBe(3);
    expect(mmToSteps(-2.54)).toBe(-2);
    expect(mmToSteps(0.5)).toBe(0);
  });
});

describe('where a part is in the file', () => {
  it('keys a part by its path from its sheet, the sheet being the first word of its path', () => {
    expect(addressOf('j1')).toEqual({ sheet: null, key: 'j1' });
    expect(addressOf('stage.output_cap')).toEqual({ sheet: 'stage', key: 'output_cap' });
    expect(addressOf('stage.filter.l1')).toEqual({ sheet: 'stage', key: 'filter.l1' });
  });
  it('keys a note by the sheet it is drawn on', () => {
    expect(keyFrom(null, 'j1')).toBe('j1');
    expect(keyFrom('stage', 'stage.output_cap')).toBe('output_cap');
    expect(keyFrom('stage', 'stage')).toBe('');
    // A relation of the board that is drawn on a module's sheet keeps its own path.
    expect(keyFrom('stage', '')).toBe('');
    expect(noteAddress('stage', 'stage.output_cap', 'doc', 1)).toEqual({ sheet: 'stage', key: 'output_cap#doc' });
    expect(noteAddress('stage', 'stage', 'relation', 1)).toEqual({ sheet: 'stage', key: '#relation' });
    expect(noteAddress('stage', 'stage', 'relation', 2)).toEqual({ sheet: 'stage', key: '#relation2' });
    expect(noteAddress(null, 'j1', 'doc', 1)).toEqual({ sheet: null, key: 'j1#doc' });
    expect(noteAddress(null, '', 'relation', 3)).toEqual({ sheet: null, key: '#relation3' });
  });
});

describe('the tables', () => {
  const t: Tables = { Trellis: tablesOf({ Trellis: board }).Trellis };
  it('are the file as the page edits it, without the lines', () => {
    expect(t.Trellis.parts.j1).toEqual({ dx: 2.54, dy: -1.27 });
    expect(t.Trellis.sheets.stage.parts['filter.l1']).toEqual({ pin: [12.7, 0] });
    expect(countIn(t, 'Trellis', null)).toBe(1);
    expect(countIn(t, 'Trellis', 'stage')).toBe(3);
    expect(countAll(t)).toBe(4);
  });
  it('set an entry without touching the others or the original', () => {
    const next = withPart(t, 'Trellis', { sheet: 'stage', key: 'output_cap' }, { dx: 1.27, dy: 0 });
    expect(partEntry(next, 'Trellis', { sheet: 'stage', key: 'output_cap' })).toEqual({ dx: 1.27, dy: 0 });
    expect(partEntry(next, 'Trellis', { sheet: null, key: 'j1' })).toEqual({ dx: 2.54, dy: -1.27 });
    expect(partEntry(t, 'Trellis', { sheet: 'stage', key: 'output_cap' })).toEqual({ rotate: 90, mirror: true });
  });
  it('make a table for a board, a sheet or an entry that is not there yet', () => {
    const next = withPart({}, 'Led', { sheet: 'power', key: 'r1' }, { dx: 1.27, dy: 1.27 });
    expect(next).toEqual({ Led: { parts: {}, notes: {}, sheets: { power: { parts: { r1: { dx: 1.27, dy: 1.27 } }, notes: {} } } } });
  });
  it('remove an entry that says nothing, and the tables that are left empty', () => {
    expect(isBlank({})).toBe(true);
    expect(isBlank({ dx: 0, dy: 0 })).toBe(true);
    expect(isBlank({ rotate: 0 })).toBe(false);
    const one = withPart({}, 'Led', { sheet: null, key: 'r1' }, { dx: 1.27, dy: 0 });
    expect(withPart(one, 'Led', { sheet: null, key: 'r1' }, null)).toEqual({});
    expect(withPart(one, 'Led', { sheet: null, key: 'r1' }, { dx: 0, dy: 0 })).toEqual({});
    expect(withNote(withNote({}, 'Led', { sheet: null, key: 'r1#doc' }, { dx: 1.27, dy: 0 }), 'Led', { sheet: null, key: 'r1#doc' }, { dx: 0, dy: 0 })).toEqual({});
  });
  it('clear a sheet, the board’s own sheet, or the whole file', () => {
    expect(countAll(withoutSheet(t, 'Trellis', 'stage'))).toBe(1);
    const own = withoutSheet(t, 'Trellis', null);
    expect(countIn(own, 'Trellis', null)).toBe(0);
    expect(countIn(own, 'Trellis', 'stage')).toBe(3);
    expect(withoutSheet(withoutSheet(t, 'Trellis', null), 'Trellis', 'stage')).toEqual({});
  });
});

describe('the text', () => {
  it('spells keys bare when it can and quoted when it must', () => {
    expect(keyText('j1')).toBe('j1');
    expect(keyText('Trellis')).toBe('Trellis');
    expect(keyText('filter.l1')).toBe('"filter.l1"');
    expect(keyText('output_cap#doc')).toBe('"output_cap#doc"');
    expect(keyText('')).toBe('""');
  });
  it('spells numbers as the server does', () => {
    expect(numberText(2.54)).toBe('2.54');
    expect(numberText(-1.27)).toBe('-1.27');
    expect(numberText(0)).toBe('0');
    expect(numberText(-0)).toBe('0');
    expect(numberText(12.7)).toBe('12.7');
    expect(numberText(3.8099999999999996)).toBe('3.81');
    expect(numberText(100)).toBe('100');
  });
  it('writes the worked example of ADR 0061, with the keys in byte order', () => {
    const text = toToml(tablesOf({ Trellis: board }));
    expect(text).toBe(`version = 1

[Trellis.parts.j1]
dx = 2.54
dy = -1.27

[Trellis.sheets.stage.parts."filter.l1"]
pin = [12.7, 0]

[Trellis.sheets.stage.parts.output_cap]
rotate = 90
mirror = true

[Trellis.sheets.stage.notes."output_cap#doc"]
dx = 5.08
dy = 0
`);
  });
  it('writes a file with nothing in it as `version = 1`', () => {
    expect(toToml({})).toBe('version = 1\n');
  });
});
