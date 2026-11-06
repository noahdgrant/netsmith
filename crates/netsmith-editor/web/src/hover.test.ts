import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { BoardDetail, PartInfo } from './api';
import {
  HOVER_DELAY_MS,
  HOVER_LEAVE_MS,
  type Hover,
  clearHover,
  hover,
  hoverAttrs,
  hoverEnter,
  hoverLeave,
  modulesLit,
  partsOfDefinition,
  resolveHover,
  sameHover,
  selectorOf,
  setHover,
} from './hover';

const at = (file: string, line: number) => ({ file, line, column: 1, endLine: line, endColumn: 10 });
const part = (refdes: string, path: string, file = 'src/main.nsm', line = 5): PartInfo => ({
  refdes,
  kind: 'Resistor',
  value: '1k',
  package: '0402',
  path,
  lcsc: null,
  assembled: true,
  decl: at(file, line),
  scopes: [],
});
const detail: BoardDetail = {
  name: 'B',
  file: 'src/main.nsm',
  notes: [],
  parts: [part('R1', 'stage1.limit', 'src/stage.nsm', 12), part('C1', 'stage1.cap', 'src/stage.nsm', 14), part('U1', 'top.inner.reg', 'src/reg.nsm', 3), part('J1', 'j1')],
  nets: [
    { name: 'GND', named: true, pins: [{ refdes: 'R1', pin: '2' }, { refdes: 'C1', pin: '2' }] },
    { name: 'RAIL', named: true, pins: [{ refdes: 'R1', pin: '1' }] },
  ],
};
const PART: Hover = { kind: 'part', key: 'stage1.limit' };
const NET: Hover = { kind: 'net', key: 'GND' };

describe('the shared hover', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    setHover(null);
  });
  afterEach(() => vi.useRealTimers());

  it('lights after a short rest, not at once', () => {
    hoverEnter(PART);
    expect(hover.value).toBeNull();
    vi.advanceTimersByTime(HOVER_DELAY_MS + 1);
    expect(hover.value).toEqual(PART);
  });
  it('goes out a moment after the pointer left', () => {
    setHover(PART);
    hoverLeave(PART);
    expect(hover.value).toEqual(PART);
    vi.advanceTimersByTime(HOVER_LEAVE_MS + 1);
    expect(hover.value).toBeNull();
  });
  it('moves from one thing to the next with no gap and no flicker', () => {
    setHover(PART);
    hoverLeave(PART);
    hoverEnter(NET);
    expect(hover.value).toEqual(NET);
    vi.advanceTimersByTime(500);
    expect(hover.value).toEqual(NET);
  });
  it('a pass over a thing that is not rested on lights nothing', () => {
    hoverEnter(PART);
    vi.advanceTimersByTime(HOVER_DELAY_MS - 20);
    hoverLeave(PART);
    vi.advanceTimersByTime(500);
    expect(hover.value).toBeNull();
  });
  it('a leave from a thing that is not the one lit changes nothing', () => {
    setHover(NET);
    hoverLeave(PART);
    vi.advanceTimersByTime(500);
    expect(hover.value).toEqual(NET);
  });
  it('Escape (clearHover) puts the light out at once and cancels what was coming', () => {
    hoverEnter(PART);
    clearHover();
    vi.advanceTimersByTime(500);
    expect(hover.value).toBeNull();
    setHover(NET);
    clearHover();
    expect(hover.value).toBeNull();
  });
  it('compares by kind and key', () => {
    expect(sameHover(PART, { kind: 'part', key: 'stage1.limit', refs: ['R1'] })).toBe(true);
    expect(sameHover(PART, { kind: 'net', key: 'stage1.limit' })).toBe(false);
    expect(sameHover(null, null)).toBe(true);
  });
});

describe('the handlers of an element', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    setHover(null);
  });
  afterEach(() => vi.useRealTimers());
  const pointer = (type: string) => ({ pointerType: type }) as PointerEvent;

  it('say what it stands for in data-hv', () => {
    expect(hoverAttrs(PART)['data-hv']).toBe('part:stage1.limit');
    expect(hoverAttrs(null)['data-hv']).toBeUndefined();
  });
  it('pointer enter and leave light and unlight; focus and blur do the same', () => {
    const a = hoverAttrs(PART);
    a.onPointerEnter(pointer('mouse'));
    vi.advanceTimersByTime(HOVER_DELAY_MS + 1);
    expect(hover.value).toEqual(PART);
    a.onPointerLeave(pointer('mouse'));
    vi.advanceTimersByTime(HOVER_LEAVE_MS + 1);
    expect(hover.value).toBeNull();
    a.onFocus();
    vi.advanceTimersByTime(HOVER_DELAY_MS + 1);
    expect(hover.value).toEqual(PART);
    a.onBlur();
    vi.advanceTimersByTime(HOVER_LEAVE_MS + 1);
    expect(hover.value).toBeNull();
  });
  it('ignore a touch', () => {
    const a = hoverAttrs(PART);
    a.onPointerEnter(pointer('touch'));
    vi.advanceTimersByTime(500);
    expect(hover.value).toBeNull();
  });
});

describe('what a hover stands for', () => {
  it('a part is itself', () => {
    const lit = resolveHover(detail, PART);
    expect([...lit.refs]).toEqual(['R1']);
    expect([...lit.paths]).toEqual(['stage1.limit']);
    expect(lit.nets.size).toBe(0);
    expect(lit.at).toBe('stage1.limit');
  });
  it('a net is its name and the parts on it', () => {
    const lit = resolveHover(detail, NET);
    expect([...lit.nets]).toEqual(['GND']);
    expect([...lit.refs].sort()).toEqual(['C1', 'R1']);
    // the drawings light the net's wires and tracks, not every part on it
    expect(lit.drawn.size).toBe(0);
    expect(lit.at).toBeNull();
  });
  it('a module is the parts under it', () => {
    const lit = resolveHover(detail, { kind: 'module', key: 'stage1' });
    expect([...lit.refs].sort()).toEqual(['C1', 'R1']);
    expect([...lit.paths].sort()).toEqual(['stage1.cap', 'stage1.limit']);
  });
  it('a bus carries its members, a sheet its designators', () => {
    const bus = resolveHover(detail, { kind: 'bus', key: 'uart', nets: ['TX', 'RX'] });
    expect([...bus.nets].sort()).toEqual(['RX', 'TX']);
    const sheet = resolveHover(detail, { kind: 'module', key: 'sheet', refs: ['U1'] });
    expect([...sheet.paths]).toEqual(['top.inner.reg']);
  });
  it('is empty before the board is read, and for nothing', () => {
    expect(resolveHover(null, PART).refs.size).toBe(0);
    expect(resolveHover(detail, null).refs.size).toBe(0);
  });
});

describe('definitions and cards', () => {
  it('the parts of a definition are those declared between its line and the next', () => {
    expect(partsOfDefinition(detail, 'src/stage.nsm', 10, 13)).toEqual(['R1']);
    expect(partsOfDefinition(detail, 'src/stage.nsm', 10, null)).toEqual(['R1', 'C1']);
    expect(partsOfDefinition(detail, 'src/other.nsm', 1, null)).toEqual([]);
    expect(partsOfDefinition(null, 'x', 1, null)).toEqual([]);
  });
  it('a part lights the nearest module around it, a module the cards below it too', () => {
    const keys = ['stage1', 'stage1.inner', 'top', 'stage2'];
    expect([...modulesLit(keys, 'stage1.inner.reg')]).toEqual(['stage1.inner']);
    expect([...modulesLit(keys, 'stage1.cap')]).toEqual(['stage1']);
    expect([...modulesLit(keys, 'stage1')].sort()).toEqual(['stage1', 'stage1.inner']);
    expect(modulesLit(keys, null).size).toBe(0);
  });
  it('one selector finds the drawings and the lists, and quotes its values', () => {
    const lit = resolveHover(detail, PART);
    const sel = selectorOf(lit, new Set(['stage1']));
    expect(sel).toContain('[data-ref="R1"]:not(.pin)');
    expect(sel).toContain('[data-hv-ref~="R1"]');
    expect(sel).toContain('[data-hv="part:stage1.limit"]');
    expect(sel).toContain('[data-hv="module:stage1"]');
    expect(selectorOf({ refs: new Set(), drawn: new Set(), paths: new Set(), nets: new Set(['a"b']), at: null }, new Set())).toContain('[data-net="a\\"b"]:not(.pin)');
  });
});
