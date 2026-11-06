import { describe, expect, it } from 'vitest';

import type { BoardDetail, Project } from './api';
import { rowOfProblem } from './feedback/problems';
import { type Index, type Item, buildIndex, fileSymbolItems, parseLine, parseQuery, problemItems, recentItems, search } from './quickopen';
import { scanSymbols } from './source/symbols';

const loc = { file: 'src/m.nsm', line: 3, column: 1, endLine: 3, endColumn: 9 };
const part = (refdes: string, path: string, value: string, kind = 'Capacitor') => ({
  refdes,
  kind,
  value,
  package: '0402',
  path,
  lcsc: null,
  assembled: true,
  decl: loc,
  scopes: path.includes('.') ? [loc] : [],
});

const detail: BoardDetail = {
  name: 'B',
  file: 'src/main.nsm',
  notes: [],
  parts: [
    part('C2', 'stage1.input_cap', '1u'),
    part('C25', 'stage2.out_cap', '22u'),
    part('C250', 'bank.c250', '10n'),
    part('R25', 'light1.limit', '1k', 'Resistor'),
    part('U1', 'stage1.regulator', 'XC6206', 'LinearRegulator'),
  ],
  nets: [
    { name: 'GND', named: true, pins: [{ refdes: 'C2', pin: '2' }] },
    { name: 'RAIL25', named: true, pins: [{ refdes: 'C25', pin: '1' }, { refdes: 'R25', pin: '1' }] },
    { name: 'net-5', named: false, pins: [] },
  ],
};
const project: Project = {
  title: 'p',
  hasManifest: true,
  kicad: false,
  folders: [],
  files: [
    { path: 'src/main.nsm', role: 'top_level', readonly: false, defs: [{ kind: 'board', name: 'BigBoard', line: 10 }] },
    { path: 'src/sheets/ldo_stage.nsm', role: 'module', readonly: false, defs: [{ kind: 'module', name: 'LdoStage', line: 8 }] },
    { path: 'std/passive.nsm', role: 'std', readonly: true, defs: [{ kind: 'part', name: 'Capacitor', line: 4 }] },
  ],
};
const index = buildIndex(detail, project);
const commands: Item[] = [
  { id: 'cmd:view.power', kind: 'command', label: 'Show Power tree', detail: 'View', fields: [{ name: 'label', text: 'Show Power tree' }], target: { type: 'command', id: 'view.power' } },
  { id: 'cmd:build', kind: 'command', label: 'Write KiCad files', detail: 'Build', fields: [{ name: 'label', text: 'Write KiCad files' }], target: { type: 'command', id: 'build' } },
];

describe('parseQuery', () => {
  it('reads the prefix mode', () => {
    expect(parseQuery('>power')).toEqual({ mode: 'command', text: 'power' });
    expect(parseQuery('@ldo')).toEqual({ mode: 'symbol', text: 'ldo' });
    expect(parseQuery('#gnd')).toEqual({ mode: 'net', text: 'gnd' });
    expect(parseQuery(':42')).toEqual({ mode: 'line', text: '42' });
    expect(parseQuery('  c25 ')).toEqual({ mode: 'all', text: 'c25' });
    expect(parseQuery('')).toEqual({ mode: 'all', text: '' });
  });
});

describe('go to anything', () => {
  it('finds a part by designator (the audit found that c25 found nothing)', () => {
    const groups = search(index, commands, 'c25');
    expect(groups[0].title).toBe('Parts');
    expect(groups[0].hits[0].item.label).toBe('C25');
    expect(groups[0].hits[0].item.target).toEqual({ type: 'selection', sel: { kind: 'part', key: 'stage2.out_cap' } });
    expect(groups[0].hits.map((h) => h.item.label)).toContain('C250');
  });
  it('finds a part by value, kind, and module path', () => {
    expect(search(index, commands, '22u')[0].hits[0].item.label).toBe('C25');
    expect(search(index, commands, 'regulator')[0].hits[0].item.label).toBe('U1');
    const byPath = search(index, commands, 'light1');
    // The module `light1` is the best match, and the part in it is listed too.
    expect(byPath[0].title).toBe('Modules');
    expect(byPath.find((g) => g.title === 'Parts')!.hits[0].item.label).toBe('R25');
  });
  it('marks the letters in the line under the name, wherever the piece sits in it', () => {
    const r = search(index, commands, 'regulator')[0].hits[0];
    expect(r.item.detail).toBe('stage1.regulator · LinearRegulator · 0402');
    // `regulator` fits the path best (positions 7..15) and the kind's end too: the best is marked.
    expect(r.marks.detail!.every((i) => i < r.item.detail.length)).toBe(true);
    const k = search(index, commands, 'linear')[0].hits[0];
    expect(k.item.label).toBe('U1');
    expect(k.marks.detail![0]).toBe('stage1.regulator · '.length);
  });
  it('shows the LCSC number it matched', () => {
    const withLcsc = buildIndex({ ...detail, parts: [{ ...detail.parts[0], lcsc: 'C25804' }] }, project);
    const hit = search(withLcsc, commands, 'c25804')[0].hits[0];
    expect(hit.item.detail.endsWith('C25804')).toBe(true);
    expect(hit.marks.detail!.length).toBe(6);
  });
  it('marks where the letters fell', () => {
    const hit = search(index, commands, 'c25')[0].hits[0];
    expect(hit.marks.label).toEqual([0, 1, 2]);
  });
  it('groups the kinds and puts the group with the best match first', () => {
    const titles = search(index, commands, 'ldo').map((g) => g.title);
    expect(titles).toContain('Definitions');
    expect(titles).toContain('Files');
    const net = search(index, commands, 'rail25');
    expect(net[0].title).toBe('Nets');
  });
  it('finds files and definitions', () => {
    const files = search(index, commands, 'ldo_stage').find((g) => g.title === 'Files')!;
    expect(files.hits[0].item.target).toEqual({ type: 'file', path: 'src/sheets/ldo_stage.nsm' });
    const defs = search(index, commands, 'ldostage').find((g) => g.title === 'Definitions')!;
    expect(defs.hits[0].item.target).toMatchObject({ type: 'definition', file: 'src/sheets/ldo_stage.nsm', line: 8, name: 'LdoStage' });
  });
  it('lists nothing for an empty query in the mixed mode (the input shows recents and tips)', () => {
    expect(search(index, commands, '')).toEqual([]);
    expect(search(index, commands, '   ')).toEqual([]);
  });
  it('finds nothing for gibberish', () => {
    expect(search(index, commands, 'qzxv')).toEqual([]);
  });
});

describe('prefix modes', () => {
  it('# lists only nets', () => {
    const g = search(index, commands, '#');
    expect(g).toHaveLength(1);
    expect(g[0].hits.map((h) => h.item.label)).toEqual(['GND', 'RAIL25', 'net-5']);
    expect(search(index, commands, '#rail')[0].hits[0].item.label).toBe('RAIL25');
    expect(search(index, commands, '#c25')).toEqual([]);
  });
  it('@ lists modules and definitions', () => {
    const g = search(index, commands, '@');
    expect(g.map((x) => x.title)).toEqual(['Modules', 'Definitions']);
    expect(g[0].hits.map((h) => h.item.label)).toEqual(['bank', 'light1', 'stage1', 'stage2']);
    expect(search(index, commands, '@stage1')[0].hits[0].item.label).toBe('stage1');
    expect(search(index, commands, '@ldostage').find((x) => x.title === 'Definitions')).toBeDefined();
  });
  it('> lists commands, ranked by what is typed', () => {
    expect(search(index, commands, '>')[0].hits).toHaveLength(2);
    expect(search(index, commands, '>kicad')[0].hits[0].item.id).toBe('cmd:build');
    expect(search(index, commands, '>pow')[0].hits[0].item.id).toBe('cmd:view.power');
  });
  it('! lists the problems, ranked by what is typed, and their rows come back by id', () => {
    const rows = [
      rowOfProblem({ severity: 'error', message: '`nonsense` is not a member of this Led', rendered: '', at: loc }, 0),
      rowOfProblem({ severity: 'warning', message: 'Led: track-width guideline: the net VCC needs 0.4 mm', rendered: '', at: null }, 1),
    ];
    const problems = problemItems(rows);
    expect(parseQuery('!nons')).toEqual({ mode: 'problem', text: 'nons' });
    expect(problems.map((i) => i.tone)).toEqual(['error', 'warning']);
    expect(problems[0].target).toEqual({ type: 'problem', id: rows[0].id });
    expect(search(index, commands, '!', problems)[0].hits).toHaveLength(2);
    expect(search(index, commands, '!vcc', problems)[0].hits[0].item.tone).toBe('warning');
    expect(search(index, commands, '!', [])).toEqual([]);
    expect(search(index, commands, '!qzxv', problems)).toEqual([]);
  });
  it(': is a line, not a search', () => {
    expect(search(index, commands, ':12')).toEqual([]);
  });
});

describe('parseLine', () => {
  it('accepts a line of the file and clamps one past the end', () => {
    expect(parseLine('12', 40)).toEqual({ line: 12, message: 'Go to line 12.' });
    expect(parseLine('99', 40).line).toBe(40);
    expect(parseLine('', 40).line).toBeNull();
    expect(parseLine('x', 40).line).toBeNull();
    expect(parseLine('0', 40).line).toBeNull();
    expect(parseLine('3', null).message).toBe('Go to line 3.');
    expect(parseLine('', null).message).toMatch(/Open a file/);
  });
});

describe('recent choices', () => {
  it('bring back the items that are still there, in order', () => {
    const got = recentItems(index, ['net:GND', 'part:gone', 'part:stage2.out_cap']);
    expect(got.map((i) => i.id)).toEqual(['net:GND', 'part:stage2.out_cap']);
  });
});

describe('buses', () => {
  const buses = [
    { path: 'mcu.uart0', type: 'Uart', nets: ['TX', 'RX', 'RTS', 'CTS'], members: [] },
    { path: 'modem.serial', type: 'Uart', nets: ['TX', 'RX', 'RTS', 'CTS'], members: [] },
    { path: 'aux.spi', type: 'Spi', nets: ['A', 'B', 'C', 'D'], members: [] },
  ];
  const withBuses = buildIndex(detail, project, buses);

  it('are named by the instance and found by it, its path or its type', () => {
    expect(withBuses.buses.map((i) => i.label)).toEqual(['uart0', 'serial', 'spi']);
    expect(withBuses.buses[0]).toMatchObject({ kind: 'bus', sub: 'Uart', detail: 'bus · 4 nets · mcu.uart0', target: { type: 'bus', path: 'mcu.uart0' } });
    const found = (q: string) => search(withBuses, commands, q).flatMap((g) => g.hits.map((h) => h.item.id));
    expect(found('uart0')).toContain('bus:mcu.uart0');
    expect(found('modem.serial')).toContain('bus:modem.serial');
    expect(found('spi')).toContain('bus:aux.spi');
  });

  it('come before the nets in the # list', () => {
    const groups = search(withBuses, commands, '#');
    expect(groups.map((g) => g.title)).toEqual(['Buses', 'Nets']);
    expect(groups[0].hits).toHaveLength(3);
    // Typing narrows them as it narrows nets.
    expect(search(withBuses, commands, '#uart').map((g) => g.title)).toEqual(['Buses']);
  });

  it('are not in the list of a board without a drawing, and can be chosen again from the recent ones', () => {
    expect(buildIndex(detail, project).buses).toEqual([]);
    expect(search(buildIndex(detail, project), commands, '#').map((g) => g.title)).toEqual(['Nets']);
    expect(recentItems(withBuses, ['bus:aux.spi', 'bus:gone']).map((i) => i.id)).toEqual(['bus:aux.spi']);
  });
});

describe('go to symbol in the open file', () => {
  const syms = scanSymbols(['module LdoStage {', '    .vin: Rail', '    .reg: LinearRegulator', '}', 'board B {', '    .stage: LdoStage', '}'].join('\n'));
  const kinds: Record<string, string> = { Rail: 'interface', LinearRegulator: 'part', LdoStage: 'module' };
  const items = fileSymbolItems('src/main.nsm', syms, (t) => kinds[t]);
  it('lists the definitions and declarations of the file, each with its line and the column of its name', () => {
    expect(items.map((i) => [i.label, i.kind, i.detail])).toEqual([
      ['LdoStage', 'definition', 'module · line 1'],
      ['.vin', 'bus', 'port · line 2'],
      ['.reg', 'part', 'part · line 3'],
      ['B', 'definition', 'board · line 5'],
      ['.stage', 'module', 'module · line 6'],
    ]);
    expect(items[2].target).toMatchObject({ type: 'symbol', file: 'src/main.nsm', line: 3, column: 6, name: 'reg' });
    expect(items[0].target).toMatchObject({ defKind: 'module' });
    expect(items[1].target).toMatchObject({ defKind: undefined });
  });
  it('is what @ shows when given, and the project\'s modules and definitions when not', () => {
    const empty: Index = { parts: [], nets: [], buses: [], modules: [], definitions: [], files: [] };
    const scoped = search(empty, [], '@reg', [], items);
    expect(scoped.map((g) => g.title)).toEqual(['In this file']);
    expect(scoped[0].hits[0].item.label).toBe('.reg');
    expect(search(empty, [], '@reg', [], null)).toEqual([]);
  });
});
