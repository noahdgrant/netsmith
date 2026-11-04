import { describe, expect, it } from 'vitest';

import type { Draw, Edge, PowerNode, PowerTree, Range } from '../api';
import { DOTS, amps, ampsText, rangeText, voltRangeText, voltsText } from './format';
import { type Item, layoutTree } from './layout';
import { TOP, issuesOf, levels, loadOf, modelOf, splitLabel, stageKind } from './model';
import { move, upstream } from './select';

const range = (lo: number, hi: number): Range => ({ lo, hi, text: `${lo}..${hi}` });
const draw = (who: string, lo: number, hi: number | null): Draw => ({
  who,
  kind: 'rail',
  range: hi === null ? null : range(lo, hi),
  at: null,
});
const node = (id: string, loads: Draw[], extra: Partial<PowerNode> = {}): PowerNode => ({
  id,
  net: id,
  board: null,
  voltage: range(3.135, 3.465),
  loads,
  supplies: [],
  demand: null,
  capacity: null,
  capacityFrom: null,
  budget: 'unlimited',
  ...extra,
});
const edge = (from: string, to: string, label: string): Edge => ({ from, to, label, kind: 'converts', at: null });

describe('numbers', () => {
  it('writes currents in A, mA and uA with three digits', () => {
    expect(ampsText(0.08401)).toBe('84 mA');
    expect(ampsText(0.002501)).toBe('2.5 mA');
    expect(ampsText(9.702e-4)).toBe('970 uA');
    expect(ampsText(1.121)).toBe('1.12 A');
    expect(ampsText(26.84e-6)).toBe('26.8 uA');
    expect(ampsText(-0.1401)).toBe('-140 mA');
  });
  it('carries into the next unit instead of writing 1000 mA', () => {
    expect(ampsText(0.9996)).toBe('1 A');
    expect(ampsText(0.99949)).toBe('999 mA');
  });
  it('writes the residue of arithmetic as zero', () => {
    expect(amps(5e-324)).toEqual({ num: '0', unit: 'A' });
    expect(ampsText(0)).toBe('0 A');
  });
  it('writes a range with thin spaces round the dots, and one amount when the ends are alike', () => {
    expect(rangeText(0.002501, 0.08401)).toBe(`2.5 mA${DOTS}84 mA`);
    expect(rangeText(0, 0)).toBe('0 A');
    expect(DOTS).toBe(' .. ');
  });
  it('writes voltages', () => {
    expect(voltsText(3.3)).toBe('3.3 V');
    expect(voltsText(0.85)).toBe('850 mV');
    expect(voltRangeText(3.135, 3.465)).toBe(`3.135${DOTS}3.465 V`);
    expect(voltRangeText(5, 5)).toBe('5 V');
  });
});

describe('loads', () => {
  it('splits a path into the part and its pin', () => {
    const l = loadOf({ who: 'mcu.pa0_ref', kind: 'rail', at: null, range: range(0, 0) });
    expect([l.part, l.pin, l.zero, l.unknown]).toEqual(['mcu', 'pa0_ref', true, false]);
    expect(loadOf({ who: 'input', kind: 'rail', at: null, range: null })).toMatchObject({ part: 'input', pin: '', unknown: true, zero: false });
  });
});

describe('a rail', () => {
  const many = [
    ...Array.from({ length: 7 }, (_, i) => draw(`p${i}.vdd`, 0, (i + 1) / 1000)),
    ...Array.from({ length: 22 }, (_, i) => draw(`mcu.pin${String(i).padStart(2, '0')}_ref`, 0, 0)),
    draw('sense.power', 0, null),
  ];
  const tree: PowerTree = { title: 't', nodes: [node('V3V3', many, { demand: range(0, 0.1), capacity: range(0, 0.2), budget: 'within' })], edges: [], stages: [] };
  const rail = modelOf(tree).rails[0];
  it('lists the five heaviest, then the rest, and the zeros apart', () => {
    expect(rail.top.map((l) => l.who)).toEqual(['p6.vdd', 'p5.vdd', 'p4.vdd', 'p3.vdd', 'p2.vdd']);
    expect(TOP).toBe(5);
    expect(rail.more.map((l) => l.who)).toEqual(['p1.vdd', 'p0.vdd']);
    expect(rail.zeros).toHaveLength(22);
    expect(rail.unknowns.map((l) => l.who)).toEqual(['sense.power']);
    expect(rail.loads).toHaveLength(30);
  });
  it('orders equal currents by name', () => {
    const t: PowerTree = { title: '', nodes: [node('A', [draw('z.p', 0, 1), draw('a.p', 0, 1), draw('m.p', 0, 2)])], edges: [], stages: [] };
    expect(modelOf(t).rails[0].top.map((l) => l.who)).toEqual(['m.p', 'a.p', 'z.p']);
  });
  it('says how full its supply is', () => {
    expect(rail.fill).toBeCloseTo(0.5);
    expect(rail.headroom).toBeCloseTo(0.1);
  });
});

describe('stages', () => {
  const tree: PowerTree = {
    title: '',
    nodes: [
      node('VIN', [draw('protect.vin', 0, 0.7)], { voltage: range(9, 13.8), supplies: [draw('input', -0.7, -0.001)] }),
      node('VPROT', [draw('buck.vin', 0, 0.7)], { voltage: range(8.6, 13.3), supplies: [draw('protect.vout', -0.7, -0.001)] }),
      node('V5', [draw('fan.power', 0, 0.2)], { voltage: range(4.85, 5.15), supplies: [draw('buck.vout', -0.2, 0)] }),
    ],
    edges: [edge('VIN', 'VPROT', 'protect (ProtectionStage)'), edge('VPROT', 'V5', 'buck (BuckConverter)')],
    stages: [
      { label: 'protect (ProtectionStage)', efficiency: null, efficiencyUnsettled: false },
      { label: 'buck (BuckConverter)', efficiency: range(0.86, 0.94), efficiencyUnsettled: false },
    ],
  };
  const m = modelOf(tree);
  it('has a kind and a title that says what it makes', () => {
    expect(m.stages.map((s) => [s.name, s.kind, s.title])).toEqual([
      ['buck', 'buck', 'Buck 5 V'],
      ['protect', 'protection', 'Protection'],
    ]);
  });
  it('shows the efficiency it has', () => {
    expect(m.stages.find((s) => s.name === 'buck')!.efficiency).toBe('86 % .. 94 %');
  });
  it('takes the converters a rail feeds out of its loads', () => {
    expect(m.rails.find((r) => r.id === 'VPROT')!.loads).toEqual([]);
    expect(m.rails.find((r) => r.id === 'VPROT')!.feeds).toEqual(['buck']);
  });
  it('links rail to stage and stage to rail', () => {
    expect(m.links).toContainEqual({ from: 'VPROT', to: 'stage:buck (BuckConverter)' });
    expect(m.links).toContainEqual({ from: 'stage:buck (BuckConverter)', to: 'V5' });
  });
  it('gets a kind from the type name, else from what the voltage does', () => {
    expect(splitLabel('ldo (LdoStage)')).toEqual({ name: 'ldo', type: 'LdoStage' });
    expect(stageKind('Regulator5V3', 'converts', range(9, 13), range(4.8, 5.2))).toBe('buck');
    expect(stageKind('X', 'converts', range(3, 3.6), range(11, 13))).toBe('boost');
    expect(stageKind('X', 'converts', range(3, 3.6), range(3, 3.6))).toBe('converter');
    expect(stageKind('Anything', 'join', null, null)).toBe('link');
  });
});

describe('issues', () => {
  const tree: PowerTree = {
    title: '',
    nodes: [
      node('OVER', [draw('a.p', 0, 0.3)], { demand: range(0, 0.3), capacity: range(0, 0.1), budget: 'over', supplies: [draw('s', -0.3, -0.1)] }),
      node('ORPHAN', [draw('b.p', 0, 0.01)], { demand: range(0, 0.01) }),
      node('UNSURE', [draw('c.p', 0, null)], { budget: 'unknown', supplies: [draw('s2', -1, -0.1)] }),
      node('FINE', [draw('d.p', 0, 0.01)], { demand: range(0, 0.01), supplies: [draw('s3', -0.1, -0.01)] }),
    ],
    edges: [],
    stages: [],
  };
  const m = modelOf(tree);
  it('lists an overdrawn supply, a rail with no source and an unsettled current, worst first', () => {
    expect(m.issues.map((i) => [i.where, i.level])).toEqual([
      ['OVER', 'bad'],
      ['ORPHAN', 'warn'],
      ['UNSURE', 'warn'],
    ]);
    expect(m.issues[0].text).toContain('300 mA');
    expect(m.issues[0].text).toContain('100 mA');
  });
  it('lists a converter whose efficiency is not settled', () => {
    const rails = m.rails;
    const stage = { id: 'stage:x', name: 'x', type: 'T', kind: 'buck' as const, title: '', from: [], to: [], at: null, efficiency: null, efficiencyUnknown: true };
    const found = issuesOf(rails, [stage]);
    expect(found.some((i) => i.id === 'stage:x' && i.level === 'warn' && i.text.includes('efficiency'))).toBe(true);
  });
  it('gives each node its worst level', () => {
    const l = levels([...m.issues, { id: 'OVER', level: 'warn', where: 'OVER', text: 'x' }]);
    expect(l.get('OVER')).toBe('bad');
    expect(l.get('ORPHAN')).toBe('warn');
    expect(l.has('FINE')).toBe(false);
  });
});

const rail = (id: string, h = 100): Item => ({ id, kind: 'rail', w: 260, h, anchor: 22 });
const stage = (id: string): Item => ({ id, kind: 'stage', w: 170, h: 60, anchor: 30 });

describe('the layout', () => {
  const items = [rail('VIN'), stage('s1'), rail('V5'), stage('s2'), rail('V3'), stage('s3'), rail('V12'), stage('s4'), rail('VA')];
  const links = [
    { from: 'VIN', to: 's1' },
    { from: 's1', to: 'V5' },
    { from: 'V5', to: 's2' },
    { from: 's2', to: 'V3' },
    { from: 'V5', to: 's3' },
    { from: 's3', to: 'V12' },
    { from: 'V3', to: 's4' },
    { from: 's4', to: 'VA' },
  ];
  const l = layoutTree(items, links);
  const at = (id: string) => l.nodes.find((n) => n.id === id)!;
  it('puts every node to the right of what feeds it', () => {
    for (const k of links) expect(at(k.to).x).toBeGreaterThan(at(k.from).x + at(k.from).w - 1);
  });
  it('alternates rails and stages in its layers', () => {
    expect(l.layers.map((layer) => layer.map((id) => at(id).kind)[0])).toEqual(['rail', 'stage', 'rail', 'stage', 'rail', 'stage', 'rail']);
  });
  it('does not overlap two nodes', () => {
    for (const a of l.nodes) {
      for (const b of l.nodes) {
        if (a === b) continue;
        const apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
        expect(apart, `${a.id} and ${b.id}`).toBe(true);
      }
    }
  });
  it('is the same every time, whatever order the nodes come in', () => {
    const again = layoutTree([...items].reverse(), [...links].reverse());
    expect(again.nodes.map((n) => [n.id, n.x, n.y]).sort()).toEqual(l.nodes.map((n) => [n.id, n.x, n.y]).sort());
    expect(again.edges.map((e) => e.d).sort()).toEqual(l.edges.map((e) => e.d).sort());
  });
  it('fits its own width and height', () => {
    for (const n of l.nodes) {
      expect(n.x + n.w).toBeLessThanOrEqual(l.width);
      expect(n.y + n.h).toBeLessThanOrEqual(l.height);
    }
    for (const e of l.edges) for (const [x, y] of e.points) expect(x >= 0 && y >= 0 && x <= l.width && y <= l.height).toBe(true);
  });
  it('draws an edge for every link, from the right of one node to the left of the next', () => {
    expect(l.edges).toHaveLength(links.length);
    const e = l.edges.find((q) => q.from === 'VIN')!;
    expect(e.points[0][0]).toBe(at('VIN').x + at('VIN').w);
    expect(e.points[e.points.length - 1][0]).toBe(at('s1').x);
  });
  it('keeps every edge out of every node', () => {
    for (const e of l.edges) {
      for (let i = 0; i + 1 < e.points.length; i++) {
        const [x1, y1] = e.points[i];
        const [x2, y2] = e.points[i + 1];
        for (const n of l.nodes) {
          if (n.id === e.from || n.id === e.to) continue;
          const hit = Math.max(x1, x2) > n.x && Math.min(x1, x2) < n.x + n.w && Math.max(y1, y2) > n.y && Math.min(y1, y2) < n.y + n.h;
          expect(hit, `${e.from}>${e.to} through ${n.id}`).toBe(false);
        }
      }
    }
  });
  it('lets a long edge pass a layer in a lane of its own', () => {
    const long = layoutTree([rail('A'), stage('s'), rail('B'), stage('t'), rail('C')], [
      { from: 'A', to: 's' },
      { from: 's', to: 'B' },
      { from: 'B', to: 't' },
      { from: 't', to: 'C' },
      { from: 'A', to: 't' },
    ]);
    const e = long.edges.find((q) => q.from === 'A' && q.to === 't')!;
    for (const n of long.nodes) {
      if (n.id === 'A' || n.id === 't') continue;
      for (let i = 0; i + 1 < e.points.length; i++) {
        const [x1, y1] = e.points[i];
        const [x2, y2] = e.points[i + 1];
        const hit = Math.max(x1, x2) > n.x && Math.min(x1, x2) < n.x + n.w && Math.max(y1, y2) > n.y && Math.min(y1, y2) < n.y + n.h;
        expect(hit, `through ${n.id}`).toBe(false);
      }
    }
  });
  it('lays out a cycle without looping, and an empty tree as nothing', () => {
    const c = layoutTree([rail('A'), stage('s'), rail('B')], [
      { from: 'A', to: 's' },
      { from: 's', to: 'B' },
      { from: 'B', to: 'A' },
    ]);
    expect(c.nodes).toHaveLength(3);
    expect(c.edges).toHaveLength(3);
    expect(layoutTree([], []).nodes).toEqual([]);
  });
});

describe('selecting', () => {
  const links = [
    { from: 'VIN', to: 's1' },
    { from: 's1', to: 'V5' },
    { from: 'V5', to: 's2' },
    { from: 's2', to: 'V3' },
    { from: 'V5', to: 's3' },
    { from: 's3', to: 'V12' },
  ];
  it('gives the chain from the sources, and only that', () => {
    const p = upstream(links, 'V3');
    expect([...p.nodes].sort()).toEqual(['V3', 'V5', 'VIN', 's1', 's2']);
    expect([...p.links].sort()).toEqual(['V5>s2', 'VIN>s1', 's1>V5', 's2>V3']);
    expect([...upstream(links, 'VIN').nodes]).toEqual(['VIN']);
  });
  it('survives a cycle', () => {
    expect(upstream([...links, { from: 'V3', to: 'VIN' }], 'V3').nodes.size).toBe(5);
  });
  it('moves along the tree with the arrow keys', () => {
    const l = layoutTree(
      [rail('VIN'), stage('s1'), rail('V5'), stage('s2'), rail('V3'), stage('s3'), rail('V12')],
      links,
    );
    expect(move(l, links, null, 'right')).toBe('VIN');
    expect(move(l, links, 'VIN', 'right')).toBe('s1');
    expect(move(l, links, 's1', 'right')).toBe('V5');
    expect(move(l, links, 'V5', 'left')).toBe('s1');
    expect(move(l, links, 'VIN', 'left')).toBeNull();
    const down = move(l, links, 'V5', 'right');
    expect(['s2', 's3']).toContain(down);
    const other = move(l, links, down, down === 's2' ? 'down' : 'up');
    expect(['s2', 's3']).toContain(other);
    expect(other).not.toBe(down);
  });
});
