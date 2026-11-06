import { describe, expect, it } from 'vitest';

import type { BoardDetail, Project } from '../api';
import { type CompletionData, offerAt, typeOptions } from './complete';
import { scanSymbols } from './symbols';

const SOURCE = ['board B {', '    .stage: LdoStage {', '    }', '    .power_in: Header1x2', '    ', '}'].join('\n');
const project: Project = {
  title: 't',
  hasManifest: true,
  kicad: false,
  folders: [],
  files: [
    { path: 'src/main.nsm', role: 'top_level', readonly: false, defs: [{ kind: 'module', name: 'LdoStage', line: 1 }, { kind: 'board', name: 'B', line: 1 }] },
    { path: 'std/parts/passive.nsm', role: 'std', readonly: true, defs: [{ kind: 'part', name: 'Resistor', line: 1 }] },
    { path: 'std/sources.nsm', role: 'std', readonly: true, defs: [{ kind: 'interface', name: 'Rail', line: 1 }] },
  ],
};
const detail: BoardDetail = {
  name: 'B',
  file: 'src/main.nsm',
  parts: [],
  nets: [{ name: '3V3', named: true, pins: [{ refdes: 'U1', pin: '2' }] }, { name: 'N$1', named: false, pins: [] }],
  notes: [],
};
const MODULE = 'module LdoStage {\n    .vin: Rail\n    .vout: Rail\n}\n';
const data = (line = 5, extra: Partial<CompletionData> = {}): CompletionData => ({
  syms: scanSymbols(SOURCE),
  line,
  project,
  detail,
  textOf: (f) => (f === 'src/main.nsm' ? MODULE : null),
  ...extra,
});
const at = (text: string, d = data(), explicit = false) => offerAt(d, text, text.length, explicit);
const labels = (o: ReturnType<typeof at>) => o?.options.map((x) => x.label);

describe('offerAt', () => {
  it('offers the members declared in the definition after a dot', () => {
    const o = at('    .')!;
    expect(o.from).toBe(5);
    expect(labels(o)).toEqual(['stage', 'power_in']);
    expect(at('    .po')!.from).toBe(5);
  });
  it('offers the members of the type of a declaration after its name and a dot', () => {
    const o = at('    .stage.v', data(2))!;
    expect(o.from).toBe(11);
    expect(labels(o)).toEqual(['vin', 'vout']);
  });
  it('offers the types of the project and the library after a name and a colon', () => {
    const o = at('    .r1: ')!;
    expect(labels(o)).toEqual(['LdoStage', 'Resistor', 'Rail']);
    expect(o.options.map((x) => x.kind)).toEqual(['module', 'part', 'interface']);
    expect(at('    .r1: Res')!.from).toBe(9);
    expect(at('    .r1: std::parts::Res')!.from).toBe('    .r1: std::parts::'.length);
  });
  it('puts the project own definitions before the library', () => {
    const o = typeOptions(project);
    expect(o.find((x) => x.label === 'LdoStage')!.boost!).toBeGreaterThan(o.find((x) => x.label === 'Resistor')!.boost!);
  });
  it('offers the named nets of the board inside the quotes of a net', () => {
    const o = at('        .pos.net &= "3')!;
    expect(labels(o)).toEqual(['3V3']);
    expect(o.from).toBe('        .pos.net &= "'.length);
  });
  it('offers units after a number once a letter is typed, or on request', () => {
    expect(at('    .current &= 5')).toBeNull();
    expect(at('    .current &= 5', data(), true)!.options.length).toBeGreaterThan(20);
    const o = at('    .current &= 5m')!;
    expect(o.from).toBe('    .current &= 5'.length);
    expect(labels(o)).toContain('mA');
  });
  it('offers the keywords at the start of a statement', () => {
    expect(labels(at('bo', data(0)))).toContain('board');
    expect(at('bo', data(0))!.from).toBe(0);
  });
  it('offers nothing in a comment, in another string, or in the middle of an expression', () => {
    expect(at('    // .')).toBeNull();
    expect(at('    .x &= "hello wor')).toBeNull();
    expect(at('    .a ~ b')).toBeNull();
  });
});
