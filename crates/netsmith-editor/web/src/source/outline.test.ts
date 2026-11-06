import { describe, expect, it } from 'vitest';

import { activeEntry, ancestorsOf, outlineRows } from './outline';
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
