import { describe, expect, it } from 'vitest';

import type { NetInfo } from '../api';
import { busChip, busCount, busMembers, busName, busTargets, busesOnNets } from './bus';
import { type Bus, busBox, busMembers as readMembers } from './drawing';
import { SCALE } from './zoom';

const bus = (path: string, sheet: string, nets: string[], flag = true): Bus => ({
  path,
  type: 'Uart',
  members: nets.map((net, i) => ({ name: ['tx', 'rx', 'rts', 'cts'][i] ?? `m${i}`, net })),
  flag,
  box: { x: 0, y: 0, w: 10, h: 10 },
  sheet,
  nets,
});
const net = (name: string, ...pins: [string, string][]): NetInfo => ({ name, named: true, pins: pins.map(([refdes, pin]) => ({ refdes, pin })) }) as NetInfo;

describe('naming', () => {
  it("is the instance's own name, and the count of nets", () => {
    expect(busName('mcu.uart0')).toBe('uart0');
    expect(busName('uart')).toBe('uart');
    expect(busCount(bus('a.b', '', ['A']))).toBe('1 net');
    expect(busChip(bus('mcu.uart0', 'mcu', ['A', 'B', 'C', 'D']))).toEqual({ title: 'bus uart0', sub: '(4 nets)' });
  });
});

describe('what the drawing says of a bus', () => {
  it('reads the members as JSON, in order, with the names a net may have', () => {
    expect(readMembers('[["tx","MODEM TX"],["rx","MODEM_RX"]]', 'x')).toEqual([
      { name: 'tx', net: 'MODEM TX' },
      { name: 'rx', net: 'MODEM_RX' },
    ]);
  });
  it('falls back to the nets of an older drawing, each named by its net', () => {
    expect(readMembers(null, 'A B')).toEqual([
      { name: 'A', net: 'A' },
      { name: 'B', net: 'B' },
    ]);
    expect(readMembers('not json', 'A')).toEqual([{ name: 'A', net: 'A' }]);
    expect(readMembers('[["tx"]]', 'A')).toEqual([{ name: 'A', net: 'A' }]);
  });

  it('has the box of the thick line and of the flag, in canvas pixels', () => {
    const line = busBox(['M10 20 L10 40 L30 40'], null)!;
    expect(line.x).toBeCloseTo(10 * SCALE);
    expect(line.y).toBeCloseTo(20 * SCALE);
    expect(line.w).toBeCloseTo(20 * SCALE);
    expect(line.h).toBeCloseTo(20 * SCALE);
    // Two runs (a spine at each end) and a flag past the end.
    const both = busBox(['M10 20 L10 40', 'M50 20 L50 40'], '50,40 52,38 60,38 60,42 52,42')!;
    expect(both.x).toBeCloseTo(10 * SCALE);
    expect(both.w).toBeCloseTo(50 * SCALE);
    expect(both.h).toBeCloseTo(22 * SCALE);
    expect(busBox([], null)).toBeNull();
    expect(busBox([''], null)).toBeNull();
  });
});

describe('the other end of a bundle flag', () => {
  // The brain's uart0 and the link's serial name their ends apart; the nets are the same.
  const partSheet = new Map([
    ['U1', 'brain'],
    ['C1', 'brain'],
    ['U2', 'link'],
    ['U3', 'board'],
    ['R1', 'extra'],
  ]);
  const nets = [
    net('TX', ['U1', '3'], ['U2', '4']),
    net('RX', ['U1', '4'], ['U2', '3']),
    net('RTS', ['U1', '5'], ['U2', '6']),
    net('CTS', ['U1', '6'], ['U2', '5']),
    // A pull-up on one of the lines on a third sheet: one net is no other end.
    net('PULL', ['R1', '1'], ['U1', '9']),
  ];
  const order = ['brain', 'link', 'board', 'extra'];
  const here = bus('brain.uart0', 'brain', ['TX', 'RX', 'RTS', 'CTS']);
  const there = bus('link.serial', 'link', ['RX', 'TX', 'CTS', 'RTS']);

  it('is the sheet that carries the member nets, whatever the other end calls them', () => {
    const t = busTargets(here, nets, partSheet, order, [here, there]);
    expect(t.map((x) => x.sheet)).toEqual(['link']);
    expect(t[0]).toMatchObject({ carried: 4, bus: there });
    expect(t[0].pins.map((p) => `${p.refdes}.${p.pin}`).sort()).toEqual(['U2.3', 'U2.4', 'U2.5', 'U2.6']);
  });

  it('is the other way too, from the modem back to the processor', () => {
    expect(busTargets(there, nets, partSheet, order, [here, there]).map((x) => [x.sheet, x.bus?.path])).toEqual([['brain', 'brain.uart0']]);
  });

  it('lists every sheet that carries them, in the strip order, and finds no bus where none is drawn', () => {
    const more = [...nets, net('TX2', ['U3', '1'])];
    const wide = [net('TX', ['U1', '3'], ['U2', '4'], ['U3', '2']), net('RX', ['U1', '4'], ['U2', '3'], ['U3', '3']), ...more.slice(2)];
    const t = busTargets(here, wide, partSheet, order, [here, there]);
    expect(t.map((x) => x.sheet)).toEqual(['link', 'board']);
    expect(t[1]).toMatchObject({ carried: 2 });
    expect(t[1].bus).toBeUndefined();
  });

  it('needs all of the nets of a bus of two, and finds nothing for a net that is not there', () => {
    const two = bus('brain.pair', 'brain', ['TX', 'RX']);
    expect(busTargets(two, nets, partSheet, order, []).map((x) => x.sheet)).toEqual(['link']);
    const lonely = busTargets(bus('brain.x', 'brain', ['PULL', 'MISSING', 'ALSO']), nets, partSheet, order, []);
    expect(lonely).toEqual([]);
  });
});

describe('the inspector rows', () => {
  it('lists each member with its net and its pins by module, in the strip order', () => {
    const partSheet = new Map([
      ['U1', 'brain'],
      ['U2', 'link'],
      ['U10', 'link'],
    ]);
    const nets = [net('TX', ['U10', '2'], ['U2', '4'], ['U1', '3']), net('RX', ['U1', '4'])];
    const rows = busMembers(bus('brain.uart0', 'brain', ['TX', 'RX']), nets, partSheet, ['brain', 'link']);
    expect(rows.map((r) => [r.name, r.net])).toEqual([
      ['tx', 'TX'],
      ['rx', 'RX'],
    ]);
    expect(rows[0].modules.map((m) => [m.sheet, m.pins.map((p) => `${p.refdes}.${p.pin}`)])).toEqual([
      ['brain', ['U1.3']],
      ['link', ['U2.4', 'U10.2']],
    ]);
    expect(rows[1].modules).toHaveLength(1);
  });
  it('has no pins for a member whose net the board no longer has', () => {
    expect(busMembers(bus('a.b', '', ['GONE']), [], new Map(), [])[0].modules).toEqual([]);
  });
});

describe('what a lit net runs through', () => {
  it('lights the buses that have any of the lit nets', () => {
    const a = bus('a.u', 's', ['A', 'B', 'C']);
    const b = bus('a.v', 's', ['D', 'E', 'F']);
    expect(busesOnNets([a, b], new Set(['B'])).map((x) => x.path)).toEqual(['a.u']);
    expect(busesOnNets([a, b], new Set())).toEqual([]);
  });
});
