import { describe, expect, it } from 'vitest';

import type { Def } from '../api';
import { fileSymbols, symsFromDefs } from './serverSyms';
import { containing, scanSymbols } from './symbols';

const TEXT = [
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
  '',
].join('\n');

const DEFS: Def[] = [
  { kind: 'module', name: 'LdoStage', line: 1, endLine: 4, children: [{ name: 'vin', kind: 'interface', typeName: 'Rail', line: 2, endLine: 2 }, { name: 'input_cap', kind: 'part', typeName: 'Capacitor', line: 3, endLine: 3 }] },
  { kind: 'board', name: 'PowerBoard', line: 6, endLine: 11, children: [{ name: 'stage', kind: 'module', typeName: 'LdoStage', line: 7, endLine: 9 }, { name: 'power_in', kind: 'part', typeName: 'Header1x2', line: 10, endLine: 10 }] },
];

describe('symsFromDefs', () => {
  it('makes the symbols the page would have read, from the server, with columns taken from the text', () => {
    const server = symsFromDefs(DEFS, TEXT)!;
    const read = scanSymbols(TEXT);
    expect(server.map((s) => [s.kind, s.name, s.line, s.endLine, s.column])).toEqual(read.map((s) => [s.kind, s.name, s.line, s.endLine, s.column]));
    expect(server[1].children.map((c) => [c.name, c.typeName, c.line, c.endLine, c.column])).toEqual(read[1].children.map((c) => [c.name, c.typeName, c.line, c.endLine, c.column]));
  });
  it('answers the breadcrumbs the same: the symbols that hold a line', () => {
    const server = symsFromDefs(DEFS, TEXT)!;
    expect(containing(server, 8).map((s) => s.name)).toEqual(['PowerBoard', 'stage']);
  });
  it('is null when the server sends no end of a definition (an older server), or no definitions', () => {
    expect(symsFromDefs(DEFS.map(({ endLine: _e, ...d }) => d), TEXT)).toBeNull();
    expect(symsFromDefs([], TEXT)).toBeNull();
    expect(symsFromDefs(undefined, TEXT)).toBeNull();
  });
  it('is null when the text is not what the server read: a name that has moved, or an end that is not a closing brace', () => {
    expect(symsFromDefs(DEFS, '\n' + TEXT)).toBeNull();
    expect(symsFromDefs(DEFS, TEXT.replace('}\n\nboard', 'x\n\nboard'))).toBeNull();
    expect(symsFromDefs(DEFS, TEXT.split('\n').slice(0, 5).join('\n'))).toBeNull();
  });
  it('is null for a child outside its definition', () => {
    const bad: Def[] = [{ ...DEFS[0], children: [{ name: 'vin', typeName: 'Rail', line: 6, endLine: 6 }] }];
    expect(symsFromDefs(bad, TEXT)).toBeNull();
  });
  it('knows a declaration of a path by its first member', () => {
    const text = ['module M {', '    .stage.regulator: LinearRegulator', '}'].join('\n');
    const s = symsFromDefs([{ kind: 'module', name: 'M', line: 1, endLine: 3, children: [{ name: 'stage.regulator', typeName: 'LinearRegulator', line: 2, endLine: 2 }] }], text)!;
    expect(s[0].children[0]).toMatchObject({ name: 'stage.regulator', column: 6 });
  });
});

describe('fileSymbols', () => {
  it('prefers the server and says so', () => {
    const r = fileSymbols(DEFS, TEXT, () => {
      throw new Error('the page must not read text the server described');
    });
    expect(r.from).toBe('server');
    expect(r.syms).toHaveLength(2);
  });
  it('keeps the server\'s answer for text appended after the last definition, which moves no line it named', () => {
    expect(fileSymbols(DEFS, `${TEXT}module Extra {\n}\n`, scanSymbols).from).toBe('server');
  });
  it('reads the text when the server has nothing that fits', () => {
    const moved = fileSymbols(DEFS, `// comment\n${TEXT}`, scanSymbols);
    expect(moved.from).toBe('text');
    expect(moved.syms.map((s) => s.name)).toEqual(['LdoStage', 'PowerBoard']);
    expect(fileSymbols(undefined, TEXT, scanSymbols).from).toBe('text');
  });
});
