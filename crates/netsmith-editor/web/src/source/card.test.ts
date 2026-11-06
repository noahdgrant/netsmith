import { describe, expect, it } from 'vitest';

import type { BoardDetail, PartInfo, PowerTree, Problem, Project } from '../api';
import { type CardContext, cardFor, definitionCard, netCard, partCard, pinsOf, targetAt } from './card';
import { scanSymbols } from './symbols';

const loc = (file: string, line: number) => ({ file, line, column: 5, endLine: line, endColumn: 30 });
const part = (refdes: string, path: string, kind: string, value: string, decl: number, scopes: number[] = [], extra: Partial<PartInfo> = {}): PartInfo => ({
  refdes,
  kind,
  value,
  package: '0402',
  path,
  lcsc: 'C100',
  assembled: true,
  decl: loc('src/main.nsm', decl),
  scopes: scopes.map((l) => loc('src/main.nsm', l)),
  ...extra,
});

// The shape of /api/board for the PowerBoard of examples/two_boards.nsm.
const detail: BoardDetail = {
  name: 'TwoBoards/power',
  file: 'src/main.nsm',
  parts: [
    part('J1', 'power_in', 'Header1x2', 'Conn_01x02', 67, [], { package: null }),
    part('C1', 'stage.input_cap', 'Capacitor', '10u', 24, [52]),
    part('C2', 'stage.output_cap', 'Capacitor', '22u', 25, [52], { package: '0603' }),
    part('U1', 'stage.regulator', 'LinearRegulator', 'XC6206P332MR-G', 21, [52], { package: 'SOT-23' }),
  ],
  nets: [
    { name: '3V3', named: true, pins: [{ refdes: 'C2', pin: '1' }, { refdes: 'U1', pin: '2' }] },
    { name: 'GND', named: true, pins: [{ refdes: 'C1', pin: '2' }, { refdes: 'C2', pin: '2' }, { refdes: 'J1', pin: '2' }, { refdes: 'U1', pin: '1' }] },
    { name: 'VIN', named: true, pins: [{ refdes: 'C1', pin: '1' }, { refdes: 'J1', pin: '1' }, { refdes: 'U1', pin: '3' }] },
  ],
  notes: [],
};

const project: Project = {
  title: 'verify',
  hasManifest: true,
  kicad: false,
  folders: [],
  files: [
    { path: 'src/main.nsm', role: 'top_level', readonly: false, defs: [{ kind: 'module', name: 'LdoStage', line: 20 }, { kind: 'board', name: 'PowerBoard', line: 45 }] },
    { path: 'std/parts/passive.nsm', role: 'std', readonly: true, defs: [{ kind: 'part', name: 'Resistor', line: 13 }, { kind: 'part', name: 'Capacitor', line: 50 }] },
  ],
};

const PASSIVE = [
  ...Array(10).fill(''),
  '/// A resistor. Its through-current and voltage drop are tied together by Ohm\'s law.',
  '/// It is never an unrated part.',
  'part Resistor {',
  '    .designator &= "R"',
  '    .resistance: Resistance',
  '    .max_power: Power',
  '    .current: Current',
  '}',
];
const texts: Record<string, string> = { 'std/parts/passive.nsm': PASSIVE.join('\n') };
const ctx = (extra: Partial<CardContext> = {}): CardContext => ({
  path: 'src/main.nsm',
  detail,
  board: 'TwoBoards/power',
  project,
  problems: [],
  power: null,
  textOf: (f) => texts[f] ?? null,
  ...extra,
});

describe('pinsOf', () => {
  it('lists the pins of a part with their nets, in pin order', () => {
    expect(pinsOf(detail, 'U1')).toEqual([
      { pin: '1', net: 'GND', named: true },
      { pin: '2', net: '3V3', named: true },
      { pin: '3', net: 'VIN', named: true },
    ]);
  });
});

describe('partCard', () => {
  it('names the part, its kind, value, package, module and pins', () => {
    const c = partCard(ctx(), detail.parts[1]);
    expect(c).toMatchObject({ refdes: 'C1', partKind: 'Capacitor', value: '10u', package: '0402', module: 'stage', lcsc: 'C100', sel: { kind: 'part', key: 'stage.input_cap' } });
    expect(c.pins.map((p) => `${p.pin}:${p.net}`)).toEqual(['1:VIN', '2:GND']);
  });
  it('has no module for a part of the board itself', () => {
    expect(partCard(ctx(), detail.parts[0]).module).toBeNull();
  });
  it('counts the problems on the declaration', () => {
    const problems: Problem[] = [{ severity: 'error', message: 'm', rendered: '', at: loc('src/main.nsm', 24) }, { severity: 'note', message: 'n', rendered: '', at: loc('src/main.nsm', 24) }];
    expect(partCard(ctx({ problems }), detail.parts[1]).problems).toBe(1);
  });
});

describe('netCard', () => {
  const power: PowerTree = {
    title: 't',
    edges: [],
    stages: [],
    nodes: [{ id: 'n', net: '3V3', board: null, voltage: { lo: 3.1, hi: 3.4, text: '3.135 V .. 3.465 V' }, loads: [{ who: 'a', kind: 'rail', range: null, at: null }], supplies: [{ who: 'b', kind: 'rail', range: null, at: null }], demand: { lo: 0, hi: 0.05, text: '0 A .. 50 mA' }, capacity: null, capacityFrom: null, budget: 'within' }],
  };
  it('gives the pins, the modules it spans and whether a part of the board is on it', () => {
    const c = netCard(ctx(), 'VIN')!;
    expect(c.pins.map((p) => `${p.refdes}.${p.pin}`)).toEqual(['C1.1', 'J1.1', 'U1.3']);
    expect(c.modules).toEqual(['stage']);
    expect(c.onBoard).toBe(true);
    expect(c.rail).toBeNull();
  });
  it('adds the rail when the power tree has a node for the net', () => {
    const c = netCard(ctx({ power }), '3V3')!;
    expect(c.rail).toEqual({ voltage: '3.135 V .. 3.465 V', supplies: 1, loads: 1, demand: '0 A .. 50 mA', capacity: null, budget: 'within' });
    expect(c.onBoard).toBe(false);
  });
  it('is null for a net the board does not have, or before the board is read', () => {
    expect(netCard(ctx(), 'NOPE')).toBeNull();
    expect(netCard(ctx({ detail: null }), 'VIN')).toBeNull();
  });
});

describe('definitionCard', () => {
  it('has the signature, the doc comment above it and the members of a library part', () => {
    const c = definitionCard(ctx(), 'Resistor')!;
    expect(c).toMatchObject({ defKind: 'part', file: 'std/parts/passive.nsm', line: 13, signature: 'part Resistor', library: true, partial: false });
    expect(c.doc).toEqual(["A resistor. Its through-current and voltage drop are tied together by Ohm's law.", 'It is never an unrated part.']);
    expect(c.members.map((m) => [m.name, m.typeName])).toEqual([['resistance', 'Resistance'], ['max_power', 'Power'], ['current', 'Current']]);
  });
  it('is partial until the text of its file is at hand', () => {
    const c = definitionCard(ctx({ textOf: () => null }), 'Resistor')!;
    expect(c.partial).toBe(true);
    expect(c.members).toEqual([]);
  });
  it('counts the parts and the ports of a definition of the project, and its problems', () => {
    const text = ['module LdoStage {', '    .vin: Rail', '    .regulator: Resistor', '    .c: Capacitor', '}'].join('\n');
    const p2: Project = { ...project, files: [{ ...project.files[0], defs: [{ kind: 'module', name: 'LdoStage', line: 1 }] }, project.files[1]] };
    const problems: Problem[] = [{ severity: 'warning', message: 'w', rendered: '', at: loc('src/main.nsm', 3) }];
    const c = definitionCard(ctx({ project: p2, problems, textOf: (f) => (f === 'src/main.nsm' ? text : texts[f] ?? null) }), 'LdoStage')!;
    expect(c).toMatchObject({ parts: 2, ports: 0, warnings: 1, errors: 0, library: false });
    // Rail is not in this project's list, so its role is unknown: a field.
    expect(c.members.map((m) => m.role)).toEqual(['field', 'part', 'part']);
  });
  it('is null for a name the project does not define', () => {
    expect(definitionCard(ctx(), 'Nothing')).toBeNull();
  });
});

const SOURCE = [
  'board PowerBoard {',
  '    .stage: LdoStage {',
  '        .vout.voltage &= 3.3V',
  '    }',
  '    .output: Rail {',
  '        .pos.net &= "3V3"',
  '    }',
  '    .input ~ .stage.vin',
  '}',
].join('\n');

describe('targetAt', () => {
  const syms = scanSymbols(SOURCE);
  const lines = SOURCE.split('\n');
  const nets = new Set(['3V3', 'VIN', 'GND']);
  const at = (line: number, word: string, nth = 0) => {
    let col = -1;
    for (let i = 0; i <= nth; i++) col = lines[line - 1].indexOf(word, col + 1);
    return targetAt(syms, lines[line - 1], line, col + 1, nets);
  };
  it('finds a declaration by its name', () => {
    const h = at(2, 'stage')!;
    expect(h.target).toMatchObject({ type: 'decl', sym: { name: 'stage' } });
    expect(lines[1].slice(h.from, h.to)).toBe('stage');
  });
  it('finds a type', () => {
    expect(at(2, 'LdoStage')!.target).toEqual({ type: 'type', name: 'LdoStage' });
    expect(at(1, 'PowerBoard')!.target).toEqual({ type: 'type', name: 'PowerBoard' });
  });
  it('finds the declaration at the root of a path, and not the members after it', () => {
    expect(at(8, 'stage')!.target).toMatchObject({ type: 'decl', sym: { name: 'stage' } });
    expect(at(8, 'vin')).toBeNull();
  });
  it('finds a string that names a net, and no other string', () => {
    expect(at(6, '"3V3"')!.target).toEqual({ type: 'net', name: '3V3' });
    expect(targetAt(syms, '    .pos.net &= "XYZ"', 6, 18, nets)).toBeNull();
  });
  it('finds nothing on whitespace or an operator', () => {
    expect(targetAt(syms, lines[1], 2, 0, nets)).toBeNull();
    expect(at(8, '~')).toBeNull();
  });
});

describe('cardFor', () => {
  it('turns a declaration that made a part into the part card', () => {
    const sym = { id: '67:power_in', name: 'power_in', kind: 'decl' as const, typeName: 'Header1x2', line: 67, endLine: 67, column: 6, depth: 1, children: [] };
    const c = cardFor(ctx(), { type: 'decl', sym, owner: null })!;
    expect(c).toMatchObject({ type: 'part', refdes: 'J1' });
  });
  it('turns a declaration that made a module into the module card, with the parts under it', () => {
    const sym = { id: '52:stage', name: 'stage', kind: 'decl' as const, typeName: 'LdoStage', line: 52, endLine: 55, column: 6, depth: 1, children: [] };
    const c = cardFor(ctx(), { type: 'decl', sym, owner: null })!;
    expect(c).toMatchObject({ type: 'module', path: 'stage', typeName: 'LdoStage' });
    expect(c.type === 'module' && c.parts.map((p) => p.refdes)).toEqual(['C1', 'C2', 'U1']);
    expect(c.type === 'module' && c.nets).toEqual(['3V3', 'GND', 'VIN']);
  });
  it('falls back to the definition of the type, with a note, when the board shown has no such part', () => {
    const sym = { id: '88:r1', name: 'r1', kind: 'decl' as const, typeName: 'Resistor', line: 88, endLine: 88, column: 6, depth: 1, children: [] };
    const c = cardFor(ctx(), { type: 'decl', sym, owner: null })!;
    expect(c).toMatchObject({ type: 'definition', name: 'Resistor' });
    expect(c.type === 'definition' && c.note).toContain('not in TwoBoards/power');
  });
  it('says the board is not built when there is none', () => {
    const sym = { id: '88:r1', name: 'r1', kind: 'decl' as const, typeName: 'Resistor', line: 88, endLine: 88, column: 6, depth: 1, children: [] };
    const c = cardFor(ctx({ detail: null, board: null }), { type: 'decl', sym, owner: null })!;
    expect(c.type === 'definition' && c.note).toContain('not built into a board yet');
  });
  it('is the net card for a net target', () => {
    expect(cardFor(ctx(), { type: 'net', name: 'VIN' })).toMatchObject({ type: 'net', name: 'VIN' });
  });
});
