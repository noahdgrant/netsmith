import { describe, expect, it } from 'vitest';

import type { Location, NetInfo } from '../api';
import { type RawDrawing, buildIndex } from './drawing';
import {
  filterSheets,
  neighbourBox,
  netNavOrder,
  netSheets,
  netSummary,
  orderSheets,
  otherSheets,
  parseTitle,
  pieceOf,
  sheetStats,
  stepRing,
} from './nav';

const at = (file: string, line: number, endLine = line): Location => ({ file, line, column: 1, endLine, endColumn: 1 });
const sheet = (name: string, x: number) => ({ name, title: `${name}: Mod`, x, y: 0, w: 100, h: 100 });

describe('titles and pieces', () => {
  it('splits a title into the instance and its module', () => {
    expect(parseTitle('adc: AdcMonitor')).toEqual({ name: 'adc', detail: 'AdcMonitor' });
    expect(parseTitle('Board')).toEqual({ name: 'Board', detail: '' });
  });
  it('knows a cut sheet and its number', () => {
    expect(pieceOf('stage (2/3)')).toEqual({ base: 'stage', piece: 2 });
    expect(pieceOf('stage')).toEqual({ base: 'stage', piece: 0 });
  });
});

describe('orderSheets', () => {
  // The drawing lists sheets alphabetically; the source declares mcu, then adc, then ldo.
  const sheets = [sheet('adc', 0), sheet('ldo', 100), sheet('mcu', 200)];
  const partSheet = new Map([
    ['U1', 'adc'],
    ['R1', 'adc'],
    ['U2', 'ldo'],
    ['U3', 'mcu'],
  ]);
  const parts = [
    { refdes: 'U1', decl: at('src/adc.nsm', 4), scopes: [at('src/main.nsm', 30)] },
    { refdes: 'R1', decl: at('src/adc.nsm', 9), scopes: [at('src/main.nsm', 30)] },
    { refdes: 'U2', decl: at('src/ldo.nsm', 4), scopes: [at('src/main.nsm', 40)] },
    { refdes: 'U3', decl: at('src/mcu.nsm', 4), scopes: [at('src/main.nsm', 12)] },
  ];
  it('follows the order the modules are declared in the source, not the alphabet', () => {
    expect(orderSheets(sheets, partSheet, parts, 'src/main.nsm').map((s) => s.name)).toEqual(['mcu', 'adc', 'ldo']);
  });
  it('is stable: a sheet with no declaration goes last in the order it came', () => {
    const more = [...sheets, sheet('zzz', 300), sheet('aaa', 400)];
    expect(orderSheets(more, partSheet, parts, 'src/main.nsm').map((s) => s.name)).toEqual(['mcu', 'adc', 'ldo', 'zzz', 'aaa']);
  });
  it('keeps the pieces of a cut sheet together, in order, where the module is declared', () => {
    const cut = [sheet('big (2/2)', 0), sheet('adc', 100), sheet('big (1/2)', 200)];
    const ps = new Map([
      ['A', 'big (1/2)'],
      ['B', 'big (2/2)'],
      ['C', 'adc'],
    ]);
    const parts2 = [
      { refdes: 'A', decl: at('src/big.nsm', 2), scopes: [at('src/main.nsm', 5)] },
      { refdes: 'B', decl: at('src/big.nsm', 3), scopes: [at('src/main.nsm', 5)] },
      { refdes: 'C', decl: at('src/adc.nsm', 2), scopes: [at('src/main.nsm', 9)] },
    ];
    expect(orderSheets(cut, ps, parts2, 'src/main.nsm').map((s) => s.name)).toEqual(['big (1/2)', 'big (2/2)', 'adc']);
  });
  it('uses a part\'s own declaration for a part in no module', () => {
    const s2 = [sheet('Board', 0), sheet('adc', 100)];
    const ps = new Map([
      ['J1', 'Board'],
      ['U1', 'adc'],
    ]);
    const parts2 = [
      { refdes: 'J1', decl: at('src/main.nsm', 50), scopes: [] },
      { refdes: 'U1', decl: at('src/adc.nsm', 2), scopes: [at('src/main.nsm', 20)] },
    ];
    expect(orderSheets(s2, ps, parts2, 'src/main.nsm').map((s) => s.name)).toEqual(['adc', 'Board']);
  });
});

describe('nets across sheets', () => {
  const partSheet = new Map([
    ['U5', 'mcu'],
    ['U6', 'mcu'],
    ['R1', 'storage'],
    ['R2', 'storage'],
    ['U9', 'adc'],
  ]);
  const net: NetInfo = {
    name: 'SPI_MOSI',
    named: true,
    pins: [
      { refdes: 'U5', pin: 'B4' },
      { refdes: 'R1', pin: '1' },
      { refdes: 'R2', pin: '2' },
      { refdes: 'U9', pin: '3' },
      { refdes: 'Q1', pin: '1' }, // not drawn
    ],
  };
  const order = ['adc', 'mcu', 'storage'];
  it('groups a net\'s pins by sheet', () => {
    const by = netSheets(net.pins, partSheet);
    expect([...by.keys()].sort()).toEqual(['adc', 'mcu', 'storage']);
    expect(by.get('storage')!.map((p) => p.refdes)).toEqual(['R1', 'R2']);
  });
  it('offers the other sheets that carry the net, in the order of the strip (a flag click)', () => {
    const t = otherSheets(net, 'mcu', partSheet, order);
    expect(t.map((x) => x.sheet)).toEqual(['adc', 'storage']);
    expect(otherSheets(net, 'adc', partSheet, ['adc', 'mcu']).map((x) => x.sheet)).toEqual(['mcu']);
    expect(otherSheets(undefined, 'mcu', partSheet, order)).toEqual([]);
    const only = { name: 'X', named: true, pins: [{ refdes: 'U5', pin: '1' }] };
    expect(otherSheets(only, 'mcu', partSheet, order)).toEqual([]);
  });
  it('says N pins on M sheets', () => {
    expect(netSummary(net, partSheet).text).toBe('5 pins on 3 sheets');
    expect(netSummary({ ...net, pins: [net.pins[0]] }, partSheet).text).toBe('1 pin on 1 sheet');
  });
  it('walks the pins sheet by sheet in the strip\'s order, then by designator as people count', () => {
    const pins = [
      { refdes: 'C10', pin: '1' },
      { refdes: 'C2', pin: '2' },
      { refdes: 'C2', pin: '1' },
      { refdes: 'U1', pin: '1' },
    ];
    const ps = new Map([
      ['C10', 'b'],
      ['C2', 'b'],
      ['U1', 'a'],
    ]);
    expect(netNavOrder(pins, ps, ['a', 'b']).map((p) => `${p.refdes}.${p.pin}`)).toEqual(['U1.1', 'C2.1', 'C2.2', 'C10.1']);
    // The strip's order, not the sheet names', decides.
    expect(netNavOrder(pins, ps, ['b', 'a'])[0].refdes).toBe('C2');
  });
  it('steps round a ring, from nothing to the first or the last', () => {
    expect(stepRing(3, 2, 1)).toBe(0);
    expect(stepRing(3, 0, -1)).toBe(2);
    expect(stepRing(3, -1, 1)).toBe(0);
    expect(stepRing(3, -1, -1)).toBe(2);
    expect(stepRing(0, -1, 1)).toBe(-1);
  });
});

describe('sheetStats', () => {
  const raw: RawDrawing = {
    sheets: [sheet('adc', 0), sheet('mcu', 100)],
    parts: [
      { ref: 'U1', box: { x: 10, y: 10, w: 20, h: 20 } },
      { ref: 'R1', box: { x: 40, y: 10, w: 10, h: 10 } },
      { ref: 'U2', box: { x: 110, y: 10, w: 20, h: 20 } },
    ],
    pins: [
      { ref: 'U1', pin: '1', net: 'A', x: 10, y: 15 },
      { ref: 'R1', pin: '1', net: 'A', x: 40, y: 15 },
      { ref: 'U2', pin: '1', net: 'A', x: 110, y: 15 },
    ],
    nets: [
      { net: 'A', kind: 'wire', box: { x: 10, y: 15, w: 30, h: 0 }, at: { x: 25, y: 15 } },
      { net: 'A', kind: 'port', box: { x: 60, y: 14, w: 10, h: 2 }, at: { x: 70, y: 15 } },
      { net: 'A', kind: 'wire', box: { x: 100, y: 15, w: 10, h: 0 }, at: { x: 105, y: 15 } },
    ],
  };
  const index = buildIndex(raw);
  it('puts each part and each net mark on the sheet whose box has it', () => {
    expect(index.partSheet.get('U1')).toBe('adc');
    expect(index.partSheet.get('U2')).toBe('mcu');
    expect([...index.netPlaces.get('A')!.keys()].sort()).toEqual(['adc', 'mcu']);
    expect(index.netPlaces.get('A')!.get('adc')!.flags).toHaveLength(1);
    expect(index.pinPoint.get('U2.1')).toMatchObject({ sheet: 'mcu' });
  });
  it('counts parts, nets, nets that leave, and problems by the declaration they are at', () => {
    const parts = [
      { refdes: 'U1', decl: at('src/adc.nsm', 4, 6), scopes: [at('src/main.nsm', 30, 30)] },
      { refdes: 'R1', decl: at('src/adc.nsm', 8), scopes: [at('src/main.nsm', 30, 30)] },
      { refdes: 'U2', decl: at('src/mcu.nsm', 4), scopes: [at('src/main.nsm', 31, 31)] },
    ];
    const stats = sheetStats(raw.sheets, index, parts, [
      { severity: 'warning', at: at('src/adc.nsm', 5) },
      { severity: 'note', at: at('src/main.nsm', 31) },
      { severity: 'error', at: null },
    ]);
    expect(stats.get('adc')).toMatchObject({ parts: 2, nets: 1, leaving: 1, warnings: 1, notes: 0 });
    expect(stats.get('mcu')).toMatchObject({ parts: 1, nets: 1, leaving: 0, warnings: 0, notes: 1, errors: 0 });
  });
});

describe('filterSheets', () => {
  const sheets = [{ ...sheet('adc', 0), title: 'adc: AdcMonitor' }, { ...sheet('modem', 100), title: 'modem: ModemModule' }];
  it('matches the name, the module, or a designator on the sheet', () => {
    const parts = (s: string) => (s === 'modem' ? ['U10', 'J2'] : ['R1']);
    expect(filterSheets(sheets, 'mode', parts).map((s) => s.name)).toEqual(['modem']);
    expect(filterSheets(sheets, 'j2', parts).map((s) => s.name)).toEqual(['modem']);
    expect(filterSheets(sheets, '', parts)).toHaveLength(2);
    expect(filterSheets(sheets, 'nothing', parts)).toEqual([]);
  });
});

describe('neighbourBox', () => {
  const box = (x: number, y: number) => ({ x, y, w: 10, h: 10 });
  const partBox = new Map([
    ['U1', box(0, 0)],
    ['C1', box(30, 0)],
    ['R1', box(0, 40)],
    ['C9', box(500, 500)],
    ['R2', box(100, 100)],
  ]);
  const partSheet = new Map([['U1', 'a'], ['C1', 'a'], ['R1', 'a'], ['C9', 'b'], ['R2', 'a']]);
  const pins = (...refs: string[]) => ({ pins: refs.map((refdes) => ({ refdes, pin: '1' })) });
  it('joins the part with the parts it shares a small net with, on its sheet', () => {
    const nets = [pins('U1', 'C1'), pins('U1', 'R1'), pins('U1', 'C9')];
    expect(neighbourBox('U1', nets, partSheet, partBox)).toEqual({ x: 0, y: 0, w: 40, h: 50 });
  });
  it('leaves out a net with many pins (ground) and a part with no box', () => {
    const big = pins('U1', 'C1', 'R1', 'R2', 'C9', 'X1', 'X2');
    expect(neighbourBox('U1', [big], partSheet, partBox)).toEqual({ x: 0, y: 0, w: 10, h: 10 });
    expect(neighbourBox('Q9', [big], partSheet, partBox)).toBeUndefined();
  });
});
