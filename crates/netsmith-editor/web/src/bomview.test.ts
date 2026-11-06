import { describe, expect, it } from 'vitest';

import type { BomLine } from './api';
import { chipMembers, designatorRanges, firstOfRange, groupLines, isProblem, moduleOfPath, sourcing } from './bomview';

const line = (over: Partial<BomLine>): BomLine => ({
  designators: ['C1'],
  quantity: 1,
  value: '10u',
  kind: 'Capacitor',
  package: '0402',
  footprint: null,
  lcsc: 'C1',
  mpn: null,
  library: 'Basic',
  unitUsd: 0.01,
  lineUsd: 0.01,
  stock: 10,
  date: '2026-10-31',
  ageDays: 5,
  stale: false,
  supply: 'file',
  unknown: null,
  unresolved: null,
  handSolder: false,
  ...over,
});

describe('designator ranges', () => {
  it('joins a run of three or more and leaves a pair alone', () => {
    expect(designatorRanges(['C1', 'C2', 'C3', 'C5', 'R1', 'R2'])).toEqual(['C1–C3', 'C5', 'R1', 'R2']);
  });
  it('sorts by letters and then by number, not as text', () => {
    expect(designatorRanges(['R10', 'R2', 'R9', 'C1'])).toEqual(['C1', 'R2', 'R9', 'R10']);
  });
  it('does not join across letters or gaps', () => {
    expect(designatorRanges(['C1', 'C2', 'R3', 'R4', 'R5'])).toEqual(['C1', 'C2', 'R3–R5']);
    expect(designatorRanges(['U1', 'U3', 'U5'])).toEqual(['U1', 'U3', 'U5']);
  });
  it('keeps a name that has no number', () => {
    expect(designatorRanges(['TP_A', 'J1'])).toEqual(['J1', 'TP_A']);
  });
  it('knows which designators a chip stands for', () => {
    expect(chipMembers('C1\u2013C3', ['C1', 'C2', 'C3', 'C5', 'R2'])).toEqual(['C1', 'C2', 'C3']);
    expect(chipMembers('C5', ['C1', 'C5'])).toEqual(['C5']);
  });
  it('names the first of a range', () => {
    expect(firstOfRange('C1–C3')).toBe('C1');
    expect(firstOfRange('C5')).toBe('C5');
  });
});

describe('sourcing', () => {
  it('says found for a priced line, stale for an old price', () => {
    expect(sourcing(line({}))).toBe('found');
    expect(sourcing(line({ stale: true }))).toBe('stale');
  });
  it('says not checked for a number without a price and date: unknown is not found', () => {
    expect(sourcing(line({ unitUsd: null, date: null }))).toBe('unchecked');
  });
  it('says ambiguous for a generic part with no row of its own, and missing for no number at all', () => {
    expect(sourcing(line({ lcsc: null, unresolved: 'a 10 k resistor: nearest rows are ...' }))).toBe('ambiguous');
    expect(sourcing(line({ lcsc: null }))).toBe('missing');
  });
  it('says by hand for a part soldered by hand with no number', () => {
    expect(sourcing(line({ lcsc: null, handSolder: true }))).toBe('hand');
  });
  it('counts ambiguous, missing and stale as problems and nothing else', () => {
    expect([line({}), line({ unitUsd: null, date: null }), line({ lcsc: null, handSolder: true })].some(isProblem)).toBe(false);
    expect([line({ stale: true }), line({ lcsc: null }), line({ lcsc: null, unresolved: 'x' })].every(isProblem)).toBe(true);
  });
});

describe('groups', () => {
  const lines = [line({ designators: ['C1', 'C2'], quantity: 2 }), line({ designators: ['R1'], quantity: 1, value: '1k' })];
  const moduleOf = (d: string) => (d === 'C2' ? 'power' : 'led');
  it('is one group with every line when not grouping', () => {
    expect(groupLines(lines, 'none', moduleOf)).toHaveLength(1);
  });
  it('groups by value', () => {
    const g = groupLines(lines, 'value', moduleOf);
    expect(g.map((x) => x.label)).toEqual(['1k', '10u']);
  });
  it('splits a line that spans two modules, each with its own designators and quantity', () => {
    const g = groupLines(lines, 'module', moduleOf);
    const led = g.find((x) => x.label === 'led')!;
    const power = g.find((x) => x.label === 'power')!;
    expect(led.lines.map((l) => l.designators)).toEqual([['C1'], ['R1']]);
    expect(power.lines.map((l) => [l.designators, l.quantity])).toEqual([[['C2'], 1]]);
  });
  it('knows the module of a path', () => {
    expect(moduleOfPath('stage.input_cap')).toBe('stage');
    expect(moduleOfPath('r1')).toBe('');
    expect(moduleOfPath(undefined)).toBe('');
  });
});
