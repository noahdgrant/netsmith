import { describe, expect, it } from 'vitest';

import { containing, docAbove, flatten, roleOf, scanCached, scanSymbols, siblings } from './symbols';

const SOURCE = `//! Two boards.

import std::parts::passive::{Capacitor, Resistor}

/// A linear regulator with a capacitor on its input and on its output.
module LdoStage {
    .vin:  Rail
    .regulator:  LinearRegulator
    .input_cap:  Capacitor { .effective_capacitance >= 1uF }

    .vin ~ .regulator.vin
}

board PowerBoard {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .stage: LdoStage {
        .vout.voltage &= 3.3V +/- 5%
    }
    .power_in: Header1x2
}

system TwoBoards {
    .power: PowerBoard
}
`;

describe('scanSymbols', () => {
  const syms = scanSymbols(SOURCE);
  it('finds the definitions, in order, with their lines', () => {
    expect(syms.map((s) => [s.kind, s.name, s.line, s.endLine])).toEqual([
      ['module', 'LdoStage', 6, 12],
      ['board', 'PowerBoard', 14, 22],
      ['system', 'TwoBoards', 24, 26],
    ]);
  });
  it('nests the declarations in their definition, with their types', () => {
    const board = syms[1];
    expect(board.children.map((c) => [c.name, c.typeName, c.line, c.endLine])).toEqual([
      ['process', 'TwoLayer', 15, 17],
      ['stage', 'LdoStage', 18, 20],
      ['power_in', 'Header1x2', 21, 21],
    ]);
    expect(syms[0].children.map((c) => c.name)).toEqual(['vin', 'regulator', 'input_cap']);
  });
  it('does not take a path or a statement for a declaration', () => {
    const names = flatten(syms).map((s) => s.name);
    expect(names).not.toContain('stackup');
    expect(names).not.toContain('vout');
  });
  it('puts the column on the name', () => {
    const stage = syms[1].children[1];
    expect(SOURCE.split('\n')[stage.line - 1].slice(stage.column - 1, stage.column - 1 + stage.name.length)).toBe('stage');
    expect(syms[0].column).toBe('module '.length + 1);
  });
  it('ignores braces in strings and comments', () => {
    const s = scanSymbols('module A {\n    // } not this\n    .x: "}" \n    .y: Foo\n}\nboard B {\n}\n');
    expect(s.map((x) => [x.name, x.endLine])).toEqual([['A', 5], ['B', 7]]);
    expect(s[0].children.map((c) => c.name)).toEqual(['x', 'y']);
  });
  it('runs an unclosed definition to the last line', () => {
    const s = scanSymbols('board A {\n    .x: Foo {\n');
    expect(s[0].endLine).toBe(3);
    expect(s[0].children[0].endLine).toBe(3);
  });
  it('reads a path type by its last word', () => {
    const s = scanSymbols('module A {\n    .x: std::parts::Foo\n}\n');
    expect(s[0].children[0].typeName).toBe('Foo');
  });
  it('is empty for no text', () => {
    expect(scanSymbols('')).toEqual([]);
  });
});

describe('containing and siblings', () => {
  const syms = scanSymbols(SOURCE);
  it('gives the chain from the definition to the innermost declaration at a line', () => {
    expect(containing(syms, 19).map((s) => s.name)).toEqual(['PowerBoard', 'stage']);
    expect(containing(syms, 21).map((s) => s.name)).toEqual(['PowerBoard', 'power_in']);
    expect(containing(syms, 14).map((s) => s.name)).toEqual(['PowerBoard']);
  });
  it('is empty between definitions', () => {
    expect(containing(syms, 4)).toEqual([]);
    expect(containing(syms, 13)).toEqual([]);
  });
  it('finds the list a symbol is in', () => {
    const stage = syms[1].children[1];
    expect(siblings(syms, stage).map((s) => s.name)).toEqual(['process', 'stage', 'power_in']);
    expect(siblings(syms, syms[2]).map((s) => s.name)).toEqual(['LdoStage', 'PowerBoard', 'TwoBoards']);
  });
});

describe('docAbove', () => {
  it('collects the doc comment directly above a line', () => {
    expect(docAbove(SOURCE.split('\n'), 6)).toEqual(['A linear regulator with a capacitor on its input and on its output.']);
  });
  it('stops at a blank line or an ordinary comment', () => {
    expect(docAbove(['/// a', '', '/// b', '// c', 'x'], 5)).toEqual([]);
    expect(docAbove(['/// a', '/// b', 'x'], 3)).toEqual(['a', 'b']);
  });
});

describe('roleOf', () => {
  const kinds: Record<string, string> = { Resistor: 'part', LdoStage: 'module', Rail: 'interface' };
  const syms = scanSymbols('board A {\n    .r: Resistor\n    .s: LdoStage\n    .p: Rail\n    .q: Resistance\n}\n');
  it('says what a declaration stands for by the kind of its type', () => {
    expect(syms[0].children.map((c) => roleOf(c, (t) => kinds[t]))).toEqual(['part', 'module', 'port', 'field']);
  });
  it('takes a generic for a part: it is declared as one (ADR 0068)', () => {
    expect(syms[0].children.map((c) => roleOf(c, (t) => (t === 'Resistor' ? 'generic' : undefined)))).toEqual(['part', 'field', 'field', 'field']);
  });
  it('knows the definitions of a generic and an enum', () => {
    const defs = scanSymbols('generic Resistor {\n    .resistance: Resistance\n}\n\nenum SpiRole {\n    Master,\n}\n');
    expect(defs.map((d) => [d.name, d.kind])).toEqual([['Resistor', 'generic'], ['SpiRole', 'enum']]);
  });
});

describe('scanCached', () => {
  it('scans a text once and gives the same symbols to the next to ask', () => {
    const a = scanCached(SOURCE);
    expect(scanCached(SOURCE)).toBe(a);
    expect(a.map((s) => s.name)).toEqual(scanSymbols(SOURCE).map((s) => s.name));
    expect(scanCached(`${SOURCE}\nboard Z {\n}\n`)).not.toBe(a);
  });
});
