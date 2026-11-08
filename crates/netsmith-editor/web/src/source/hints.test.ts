import { describe, expect, it } from 'vitest';

import type { BoardDetail, PartInfo, PowerTree } from '../api';
import { inlineHints, netHints, partHints, partText } from './hints';

const at = (file: string, line: number) => ({ file, line, column: 5, endLine: line, endColumn: 20 });
const part = (refdes: string, path: string, value: string, line: number, extra: Partial<PartInfo> = {}): PartInfo => ({
  refdes,
  kind: 'Capacitor',
  value,
  package: '0402',
  path,
  lcsc: 'C1',
  assembled: true,
  decl: at('src/main.nsm', line),
  scopes: [],
  ...extra,
});
const detail = (parts: PartInfo[]): BoardDetail => ({ name: 'B', file: 'src/main.nsm', parts, nets: [], notes: [] });

describe('partText', () => {
  it('joins value, package and the supplier number, leaving out what is missing', () => {
    expect(partText(part('C1', 'c', '10u', 1))).toBe('10u · 0402 · C1');
    expect(partText(part('J1', 'j', 'Conn', 1, { package: null, lcsc: null }))).toBe('Conn');
  });
});

describe('partHints', () => {
  it('gives one hint for each declared part of the file, on its line', () => {
    const d = detail([part('C1', 'a.c', '10u', 24), part('R1', 'r', '1k', 91, { lcsc: null })]);
    expect(partHints(d, 'src/main.nsm').map((h) => [h.line, h.text])).toEqual([
      [24, '10u · 0402 · C1'],
      [91, '1k · 0402'],
    ]);
  });
  it('leaves out the parts of other files', () => {
    const d = detail([part('C1', 'c', '10u', 3, { decl: at('std/parts/passive.nsm', 3) })]);
    expect(partHints(d, 'src/main.nsm')).toEqual([]);
  });
  it('folds the instances of one declaration: the shared text and a count, or that they differ', () => {
    const same = detail([part('C1', 'a.c', '10u', 24), part('C3', 'b.c', '10u', 24)]);
    expect(partHints(same, 'src/main.nsm')[0].text).toBe('10u · 0402 · C1 ×2');
    const differ = detail([part('C1', 'a.c', '10u', 24), part('C3', 'b.c', '22u', 24)]);
    expect(partHints(differ, 'src/main.nsm')[0].text).toBe('2 instances, 2 values');
    expect(partHints(differ, 'src/main.nsm')[0].title).toBe('C1: 10u · 0402 · C1\nC3: 22u · 0402 · C1');
  });
  it('is empty without a board', () => {
    expect(partHints(null, 'src/main.nsm')).toEqual([]);
  });
});

const range = (text: string) => ({ lo: 0, hi: 0, text });
const power = (nodes: Partial<PowerTree['nodes'][number]>[]): PowerTree => ({
  title: 't',
  edges: [],
  stages: [],
  issues: [],
  nodes: nodes.map((n, i) => ({ id: `n${i}`, net: 'X', board: null, voltage: null, loads: [], supplies: [], demand: null, capacity: null, capacityFrom: null, budget: 'unknown' as const, ...n })),
});

describe('netHints', () => {
  const lines = ['board B {', '    .pos.net &= "3V3"', '    .neg.net &= "GND"', '}'];
  it('shows the solved voltage of a rail where the source names its net', () => {
    // the server's text has no space before the unit; the hint shows a thin one
    const p = power([{ net: '3V3', voltage: range('3.135V..3.465V'), demand: range('0A..50mA') }, { net: 'GND', voltage: null }]);
    expect(netHints(lines, p)).toEqual([{ line: 2, text: '3.135\u2009V\u2009..\u20093.465\u2009V', title: '3V3: 3.135\u2009V\u2009..\u20093.465\u2009V, draws 0\u2009A\u2009..\u200950\u2009mA' }]);
  });
  it('is empty before the power tree is read', () => {
    expect(netHints(lines, null)).toEqual([]);
  });
});

describe('inlineHints', () => {
  it('merges both kinds in line order, a part winning on the same line, and drops lines past the end', () => {
    const lines = ['a', '    .pos.net &= "3V3"', 'c'];
    const p = power([{ net: '3V3', voltage: range('3.3 V') }]);
    const d = detail([part('C1', 'c', '10u', 2), part('C2', 'd', '1u', 9)]);
    expect(inlineHints('src/main.nsm', lines, d, p).map((h) => [h.line, h.text])).toEqual([[2, '10u · 0402 · C1']]);
    expect(inlineHints('src/main.nsm', lines, null, p).map((h) => h.line)).toEqual([2]);
  });
});
