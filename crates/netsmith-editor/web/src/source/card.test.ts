import { describe, expect, it } from 'vitest';

import type { BoardDetail, ModuleUse, PartInfo, PowerTree, Problem, Project, Solved } from '../api';
import { type CardContext, boardsUsing, cardFor, definitionCard, netCard, offersBoards, partCard, pinsOf, rangeText, solvedRows, targetAt, valueFromText } from './card';
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
  it('says which statement gave the package, and nothing when none did (issue 625)', () => {
    const from = { at: loc('src/main.nsm', 6), how: 'boardDefault' as const };
    const c = partCard(ctx(), { ...detail.parts[1], packageFrom: from });
    expect(c.packageFrom).toEqual(from);
    expect(valueFromText(from)).toBe('board default, main.nsm:6');
    expect(valueFromText({ ...from, how: 'default' })).toBe('default, main.nsm:6');
    expect(valueFromText({ ...from, how: 'stated' })).toBe('stated, main.nsm:6');
    expect(partCard(ctx(), detail.parts[1]).packageFrom).toBeNull();
  });
});

describe('netCard', () => {
  const power: PowerTree = {
    title: 't',
    edges: [],
    stages: [],
    issues: [],
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

describe('a pin of a net names itself the way the datasheet does', () => {
  const named: BoardDetail = {
    ...detail,
    nets: [{ name: '3V3', named: true, pins: [{ refdes: 'C2', pin: '1' }, { refdes: 'U1', pin: '2', name: 'VOUT' }, { refdes: 'U2', pin: '7', name: 'PB6', function: 'UART1_TX' }] }],
  };
  it('carries the pin name and the chosen function on the net card, and leaves them off a pin without', () => {
    const c = netCard(ctx({ detail: named }), '3V3')!;
    expect(c.pins).toEqual([
      { refdes: 'C2', pin: '1' },
      { refdes: 'U1', pin: '2', name: 'VOUT' },
      { refdes: 'U2', pin: '7', name: 'PB6', function: 'UART1_TX' },
    ]);
  });
  it('carries them on the pins of a part card too', () => {
    expect(pinsOf(named, 'U2')).toEqual([{ pin: '7', net: '3V3', named: true, name: 'PB6', function: 'UART1_TX' }]);
  });
});

describe('the solved block of a part', () => {
  const solved: Solved = {
    effectiveCapacitance: { lo: 1.72e-6, hi: 2.916e-6, text: '1.72uF..2.916uF' },
    bias: { lo: 5, hi: 5, text: '5V' },
    tolerance: { percent: 20, text: '+/- 20%' },
    ratings: [{ name: 'max_voltage', range: { lo: 0, hi: 16, text: '0V..16V' } }],
    row: { catalog: 'capacitor', mpn: 'CL05A106MQ5NUNC', lcsc: 'C315248', library: 'Basic', generic: false },
  };
  it('is on the part card when the server sent it, and null when it did not', () => {
    const c = partCard(ctx(), { ...detail.parts[1], solved });
    expect(c.solved).toBe(solved);
    expect(partCard(ctx(), detail.parts[1]).solved).toBeNull();
  });
  it('reads as the effective capacitance at its bias, the tolerance, the ratings and the catalog row, with thin spaces', () => {
    expect(solvedRows(solved)).toEqual([
      { label: 'In use', value: '1.72\u2009uF\u2009..\u20092.916\u2009uF', mono: true },
      { label: 'Bias', value: '5\u2009V', mono: true },
      { label: 'Tolerance', value: '+/- 20\u2009%' },
      { label: 'Rated', value: 'max voltage 0\u2009V\u2009..\u200916\u2009V' },
      { label: 'Row', value: 'capacitor · CL05A106MQ5NUNC · C315248 · Basic', mono: true },
    ]);
  });
  it('says a generic part has no catalog row yet', () => {
    const rows = solvedRows({ row: { catalog: 'resistor', generic: true } });
    expect(rows).toEqual([{ label: 'Row', value: 'generic resistor: a value and a package, no catalog row yet', mono: false }]);
  });
  it('has no rows for a part the solver had nothing to say about', () => {
    expect(solvedRows(null)).toEqual([]);
    expect(solvedRows(undefined)).toEqual([]);
    expect(solvedRows({})).toEqual([]);
  });
  it('shows a range with a space before the unit', () => {
    expect(rangeText({ lo: 3.135, hi: 3.465, text: '3.135V..3.465V' })).toBe('3.135\u2009V\u2009..\u20093.465\u2009V');
  });
});

describe('a definition card from the server', () => {
  const served = (extra: object = {}) => ({ at: { file: 'std/parts/passive.nsm', line: 13, column: 6, endLine: 13, endColumn: 14 }, signature: 'part Resistor { .resistance: Resistance .max_power: Power ... }', doc: "A resistor.\nIt is never an unrated part.", ...extra });
  const withEnds: Project = {
    ...project,
    files: [
      project.files[0],
      { ...project.files[1], defs: [{ kind: 'part', name: 'Resistor', line: 13, endLine: 18, children: [{ name: 'resistance', typeName: 'Resistance', line: 15, endLine: 15 }, { name: 'dissipation', kind: 'interface', typeName: 'Heat', line: 16, endLine: 16 }] }, project.files[1].defs[1]] },
    ],
  };
  it('takes the signature and the doc from /api/definition, and needs no library file', () => {
    const c = definitionCard(ctx({ project: withEnds, served: served(), textOf: () => null }), 'Resistor')!;
    expect(c).toMatchObject({ signature: 'part Resistor { .resistance: Resistance .max_power: Power ... }', source: 'server', partial: false });
    expect(c.doc).toEqual(['A resistor.', 'It is never an unrated part.']);
  });
  it('takes the members and the roles from the file list (Def.children), and the problems from the end line', () => {
    const problems: Problem[] = [{ severity: 'error', message: 'm', rendered: '', at: loc('std/parts/passive.nsm', 17) }];
    const c = definitionCard(ctx({ project: withEnds, served: served(), problems, textOf: () => null }), 'Resistor')!;
    expect(c.members.map((m) => [m.name, m.typeName, m.role])).toEqual([['resistance', 'Resistance', 'field'], ['dissipation', 'Heat', 'port']]);
    expect(c.errors).toBe(1);
  });
  it('ignores an answer that is about another place than the file list gives', () => {
    const c = definitionCard(ctx({ project: withEnds, served: served({ at: { file: 'std/parts/passive.nsm', line: 50, column: 1, endLine: 50, endColumn: 2 } }), textOf: () => null }), 'Resistor')!;
    expect(c.signature).toBe('part Resistor');
    expect(c.source).toBe('none');
  });
  it('ignores an answer of an older server (a place alone) and reads the text, as it did', () => {
    const c = definitionCard(ctx({ served: { at: served().at } }), 'Resistor')!;
    expect(c).toMatchObject({ signature: 'part Resistor', source: 'text' });
    expect(c.doc[0]).toContain('A resistor.');
  });
  it('is not partial when the file list has the ends, before the answer arrives, but says there is nothing yet', () => {
    const c = definitionCard(ctx({ project: withEnds, textOf: () => null }), 'Resistor')!;
    expect(c).toMatchObject({ partial: false, source: 'none', doc: [] });
  });
});

describe('the boards that use a module', () => {
  const uses: ModuleUse[] = [
    { name: 'LdoStage', file: 'src/main.nsm', line: 20, usedBy: ['TwoBoards/power', 'TwoBoards/spare'] },
    { name: 'Filter', file: 'src/filter.nsm', line: 3, usedBy: ['TwoBoards/power'] },
  ];
  it('are found by the file and the line of the definition the declaration is in', () => {
    expect(boardsUsing(uses, 'src/main.nsm', 20)).toEqual(['TwoBoards/power', 'TwoBoards/spare']);
    expect(boardsUsing(uses, 'src/main.nsm', 45)).toEqual([]);
    expect(boardsUsing(uses, 'src/filter.nsm', 20)).toEqual([]);
  });
  it('are none for a declaration outside a definition, and for a server that does not say', () => {
    expect(boardsUsing(uses, 'src/main.nsm', null)).toEqual([]);
    expect(boardsUsing(undefined, 'src/main.nsm', 20)).toEqual([]);
    expect(boardsUsing(null, 'src/main.nsm', 20)).toEqual([]);
  });
  it('are offered when several boards share the module, or the board shown is not one of them', () => {
    expect(offersBoards(['a', 'b'], 'a')).toBe(true);
    expect(offersBoards(['b'], 'a')).toBe(true);
    expect(offersBoards(['a'], 'a')).toBe(false);
    expect(offersBoards([], 'a')).toBe(false);
  });
});
