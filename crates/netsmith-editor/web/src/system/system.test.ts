import { describe, expect, it } from 'vitest';

import type { Draw, PowerNode, PowerTree, SystemNode, SystemView } from '../api';
import { layoutTree } from '../flow/layout';
import { CARD_W, ANCHOR, STEP, cardHeight, labelWidth } from './metrics';
import { modelOf, parseQuantity, portRow, shorten } from './model';

const port = (name: string, voltage: string | null = null, current: string | null = null) => ({ name, voltage, current });
const node = (id: string, ports: SystemNode['ports'] = [], kind = 'part'): SystemNode => ({ id, name: id, typeName: id.toUpperCase(), kind, ports, at: null });
const edge = (from: string, to: string, label: string) => ({ from, to, label, kind: 'net' as const, at: null });
const draw = (who: string): Draw => ({ who, kind: 'rail', range: null, at: null });
const rail = (net: string, supplier: string): PowerNode => ({
  id: net, net, board: null, voltage: null, loads: [], supplies: [draw(`${supplier}.out`)], demand: null, capacity: null, capacityFrom: null, budget: 'unlimited',
});
const view = (nodes: SystemNode[], edges: ReturnType<typeof edge>[]): SystemView => ({ name: 'T', kind: 'board', nodes, edges, mates: [], groundGroups: [] });
const tree = (nodes: PowerNode[]): PowerTree => ({ title: 'T', nodes, edges: [], stages: [], issues: [] });

describe('port values', () => {
  it('reads the numbers of a quantity in base units', () => {
    expect(parseQuantity('3.234V..3.333V')).toEqual([3.234, 3.333]);
    expect(parseQuantity('-140.1mA..0A')?.map((x) => Number(x.toPrecision(4)))).toEqual([-0.1401, 0]);
    expect(parseQuantity('1.2uA')).toEqual([1.2e-6, 1.2e-6]);
    expect(parseQuantity(null)).toBeNull();
    expect(parseQuantity('a name')).toBeNull();
  });
  it('writes a short value and marks a supply', () => {
    const r = portRow(port('vdd', '3.234V..3.333V', '0A..100mA'));
    expect(r.value).toBe('3.284 V  0 A .. 100 mA');
    expect(r.supply).toBe(false);
    expect(r.title).toContain('3.234V..3.333V');
    expect(portRow(port('out', '3.3V', '-140mA..0A')).supply).toBe(true);
    expect(portRow(port('uart0')).value).toBe('');
  });
});

describe('the wires', () => {
  const nodes = [node('reg'), node('mcu'), node('flash'), node('modem'), node('lone')];
  const edges = [
    edge('mcu', 'reg', 'V3V3'),
    edge('flash', 'mcu', 'SPI_SCLK'),
    edge('mcu', 'flash', 'SPI_MOSI'),
    edge('mcu', 'flash', 'SPI_MISO'),
    edge('mcu', 'flash', 'SPI_CS'),
    edge('modem', 'mcu', 'TX'),
  ];
  const m = modelOf(view(nodes, edges), tree([rail('V3V3', 'reg')]));
  it('runs power from what supplies to what is supplied, whichever way the edge came', () => {
    const w = m.wires.find((x) => x.labels.includes('V3V3'))!;
    expect([w.from, w.to, w.kind]).toEqual(['reg', 'mcu', 'power']);
  });
  it('joins the nets between two members into one wire with a count', () => {
    const w = m.wires.find((x) => x.from === 'flash' || x.to === 'flash')!;
    expect(w.labels).toEqual(['SPI_CS', 'SPI_MISO', 'SPI_MOSI', 'SPI_SCLK']);
    expect(w.text).toBe('4 nets');
    expect(m.wires.find((x) => x.labels.includes('TX'))!.text).toBe('TX');
    expect(m.wires).toHaveLength(3);
  });
  it('puts the hub of the signals to the right of what has fewer connections', () => {
    const signal = m.wires.filter((x) => x.kind === 'signal');
    for (const w of signal) expect(w.to).toBe('mcu');
  });
  it('counts the wires that end on a member', () => {
    expect(m.modules.find((x) => x.id === 'mcu')!.wires).toBe(3);
    expect(m.modules.find((x) => x.id === 'lone')!.wires).toBe(0);
  });
  it('knows which end of a join supplies: the board port that others draw from', () => {
    const v: SystemView = {
      name: 'S', kind: 'system',
      nodes: [node('led', [port('input', '3.3V', '-16mA..-7mA')], 'board'), node('power', [port('output', '3.3V', '0A..50mA')], 'board')],
      edges: [{ from: 'led', to: 'power', label: 'led.input ~ power.output', kind: 'join', at: null }],
      mates: [], groundGroups: [],
    };
    const w = modelOf(v, null).wires[0];
    expect([w.from, w.to, w.kind, w.text]).toEqual(['power', 'led', 'power', 'led.input ~ power.output']);
  });
  it('is the same whatever order the edges come in', () => {
    const again = modelOf(view([...nodes].reverse(), [...edges].reverse()), tree([rail('V3V3', 'reg')]));
    expect(again.wires).toEqual(m.wires);
  });
  it('cuts a long label for the drawing and keeps it for the tooltip', () => {
    expect(shorten('SUPERCAP_ADC_REFERENCE_PIN_LONG', 12)).toBe('SUPERCAP_AD…');
    expect(shorten('abc', 12)).toBe('abc');
    expect(labelWidth('abcd')).toBeGreaterThan(labelWidth('ab'));
  });
});

describe('the cards', () => {
  const ports = (n: number) => Array.from({ length: n }, (_, i) => portRow(port(`p${i}`)));
  const mod = (n: number) => ({ id: 'm', name: 'm', type: 'T', kind: 'part', at: null, ports: ports(n), wires: 0 });
  it('shows few ports in full and folds many into one line', () => {
    expect(cardHeight(mod(3), false, { in: 0, out: 0 })).toBeLessThan(cardHeight(mod(6), false, { in: 0, out: 0 }));
    expect(cardHeight(mod(22), false, { in: 0, out: 0 })).toBeLessThan(cardHeight(mod(6), false, { in: 0, out: 0 }) + 40);
    expect(cardHeight(mod(22), true, { in: 0, out: 0 })).toBeGreaterThan(cardHeight(mod(22), false, { in: 0, out: 0 }) + 21 * 20);
  });
  it('is tall enough for the ends of its edges', () => {
    expect(cardHeight(mod(0), false, { in: 0, out: 9 })).toBeGreaterThanOrEqual(9 * STEP);
  });
  it('lays out with every label clear and no edge through a card', () => {
    const items = ['a', 'b', 'c', 'd'].map((id) => ({ id, kind: 'module', w: CARD_W, h: cardHeight(mod(5), false, { in: 3, out: 3 }), anchor: ANCHOR, portStep: STEP }));
    const links = [
      { from: 'a', to: 'b', label: '4 nets', labelW: labelWidth('4 nets') },
      { from: 'a', to: 'c', label: 'SUPERCAP_ADC', labelW: labelWidth('SUPERCAP_ADC') },
      { from: 'b', to: 'd', label: 'UART0_TX', labelW: labelWidth('UART0_TX') },
      { from: 'a', to: 'd', label: '+VBATT_SAFE', labelW: labelWidth('+VBATT_SAFE') },
      { from: 'd', to: 'a', label: 'back', labelW: labelWidth('back') },
    ];
    const l = layoutTree(items, links);
    const boxes = l.edges.map((e) => ({ id: e.from + e.to, x: e.label!.x, y: e.label!.y - 13, w: e.label!.w, h: 15 }));
    for (const a of boxes) {
      for (const b of boxes) if (a.id < b.id) expect(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y, `${a.id} on ${b.id}`).toBe(true);
      for (const n of l.nodes) expect(a.x + a.w <= n.x || n.x + n.w <= a.x || a.y + a.h <= n.y || n.y + n.h <= a.y, `${a.id} on ${n.id}`).toBe(true);
    }
  });
});
