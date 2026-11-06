import { describe, expect, it } from 'vitest';

import { NO_CHOICE, activeEntry, ancestorsOf, allChoice, foldKeys, foldedIds, holds, outlineRows, toggled } from './outline';
import { scanSymbols } from './symbols';

const SOURCE = [
  'module LdoStage {',
  '    .vin: Rail',
  '    .input_cap: Capacitor',
  '}',
  '',
  'board PowerBoard {',
  '    .stage: LdoStage {',
  '        .vout.voltage &= 3.3V',
  '    }',
  '    .power_in: Header1x2',
  '}',
].join('\n');
const kinds: Record<string, string> = { Rail: 'interface', Capacitor: 'part', LdoStage: 'module', Header1x2: 'part' };
const base = { kindOf: (t: string) => kinds[t], filter: '', folded: new Set<string>(), problems: [] };

describe('outlineRows', () => {
  const syms = scanSymbols(SOURCE);
  it('lists the definitions with what they declare, nested by level', () => {
    const rows = outlineRows(syms, base);
    expect(rows.map((r) => [r.level, r.sym.name, r.role])).toEqual([
      [0, 'LdoStage', 'definition'],
      [1, 'vin', 'port'],
      [1, 'input_cap', 'part'],
      [0, 'PowerBoard', 'definition'],
      [1, 'stage', 'module'],
      [1, 'power_in', 'part'],
    ]);
  });
  it('marks a row with children open or folded, and a leaf with null', () => {
    const rows = outlineRows(syms, base);
    expect(rows.map((r) => r.open)).toEqual([true, null, null, true, null, null]);
    const folded = outlineRows(syms, { ...base, folded: new Set([syms[1].id]) });
    expect(folded.map((r) => r.sym.name)).toEqual(['LdoStage', 'vin', 'input_cap', 'PowerBoard']);
    expect(folded[3].open).toBe(false);
  });
  it('filters by name or type and keeps the parents of a match', () => {
    const rows = outlineRows(syms, { ...base, filter: 'cap' });
    expect(rows.map((r) => r.sym.name)).toEqual(['LdoStage', 'input_cap']);
    expect(rows[1].mark).toEqual([6, 9]);
    const byType = outlineRows(syms, { ...base, filter: 'header' });
    expect(byType.map((r) => r.sym.name)).toEqual(['PowerBoard', 'power_in']);
  });
  it('ignores the folds while filtering', () => {
    const rows = outlineRows(syms, { ...base, filter: 'power', folded: new Set([syms[1].id]) });
    expect(rows.map((r) => r.sym.name)).toEqual(['PowerBoard', 'power_in']);
  });
  it('counts the errors and warnings that fall in each symbol, parents included', () => {
    const rows = outlineRows(syms, {
      ...base,
      problems: [
        { line: 3, endLine: 3, severity: 'error' },
        { line: 8, endLine: 8, severity: 'warning' },
        { line: 8, endLine: 8, severity: 'note' },
      ],
    });
    const by = Object.fromEntries(rows.map((r) => [r.sym.name, [r.errors, r.warnings]]));
    expect(by).toEqual({ LdoStage: [1, 0], vin: [0, 0], input_cap: [1, 0], PowerBoard: [0, 1], stage: [0, 1], power_in: [0, 0] });
  });
});

describe('the entry of the cursor', () => {
  const syms = scanSymbols(SOURCE);
  it('is the innermost symbol that holds the line', () => {
    expect(syms.length).toBe(2);
    expect(activeEntry(syms, 8)).toBe(syms[1].children[0].id);
    expect(activeEntry(syms, 6)).toBe(syms[1].id);
    expect(activeEntry(syms, 5)).toBeNull();
    expect(activeEntry(syms, null)).toBeNull();
  });
  it('lists the ancestors of an entry, to open them', () => {
    expect(ancestorsOf(syms, syms[1].children[0].id)).toEqual([syms[1].id]);
    expect(ancestorsOf(syms, syms[0].id)).toEqual([]);
  });
});

describe('which definitions are folded', () => {
  const syms = scanSymbols(SOURCE);
  const [ldo, board] = syms;
  it('is every definition that does not hold the cursor, the one that does stays open', () => {
    expect([...foldedIds(syms, 7)]).toEqual([ldo.id]);
    expect([...foldedIds(syms, 2)]).toEqual([board.id]);
    expect([...foldedIds(syms, null)].sort()).toEqual([ldo.id, board.id].sort());
    expect([...foldedIds(syms, 5)].sort()).toEqual([ldo.id, board.id].sort());
  });
  it('lets the person\'s choice win over where the cursor is', () => {
    const keys = foldKeys(syms);
    const openLdo = toggled(NO_CHOICE, keys.get(ldo.id)!, true);
    expect([...foldedIds(syms, 7, openLdo)]).toEqual([]);
    const closeBoard = toggled(NO_CHOICE, keys.get(board.id)!, false);
    expect([...foldedIds(syms, 7, closeBoard)].sort()).toEqual([ldo.id, board.id].sort());
  });
  it('is kept by names, so an edit that moves the lines changes nothing', () => {
    const moved = scanSymbols(`// a\n// b\n${SOURCE}`);
    expect([...foldKeys(moved).values()]).toEqual([...foldKeys(syms).values()]);
  });
  it('folds a declaration only when the person did', () => {
    const keys = foldKeys(syms);
    const stage = board.children.find((c) => c.name === 'stage')!;
    expect(stage.children.length).toBe(0);
    const deep = scanSymbols(['board B {', '    .a: X {', '        .b: Y {', '            .c &= 1', '        }', '    }', '}'].join('\n'));
    const a = deep[0].children[0];
    expect(a.children.length).toBeGreaterThan(0);
    expect(foldedIds(deep, 1).has(a.id)).toBe(false);
    const closed = toggled(NO_CHOICE, foldKeys(deep).get(a.id)!, false);
    expect(foldedIds(deep, 1, closed).has(a.id)).toBe(true);
    void keys;
  });
  it('has a pair of choices that open or fold every entry with children', () => {
    const all = allChoice(syms, false);
    expect([...foldedIds(syms, null, all)]).toEqual([]);
    const none = allChoice(syms, true);
    expect([...foldedIds(syms, 7, none)].sort()).toEqual([ldo.id, board.id].sort());
  });
  it('counts what a definition holds', () => {
    expect(holds(ldo)).toBe(2);
    expect(holds(board)).toBe(2);
  });
  it('shows a folded definition as a row with its children hidden', () => {
    const rows = outlineRows(syms, { ...base, folded: foldedIds(syms, 2) });
    expect(rows.map((r) => r.sym.name)).toEqual(['LdoStage', 'vin', 'input_cap', 'PowerBoard']);
    expect(rows[3].open).toBe(false);
  });
});
