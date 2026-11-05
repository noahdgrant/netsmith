import { describe, expect, it } from 'vitest';

import type { FabReport, PcbReport } from '../api';
import { buildResult, fabResult, orderResult, routeResult } from './results';

const fab = (over: Partial<FabReport>): FabReport => ({
  board: 'Led',
  ok: true,
  drc: 'passed',
  summary: '',
  notes: '',
  output: '',
  downloads: [{ key: 'gerbers', name: 'gerbers.zip' }],
  elapsedMs: 5100,
  ...over,
});

describe('fabResult', () => {
  it('says what was made, how long it took, and offers the Gerbers', () => {
    const r = fabResult(fab({}));
    expect(r.kind).toBe('ok');
    expect(r.title).toBe('Fab files made for Led');
    expect(r.detail).toContain('took 5.1 s');
    expect(r.detail).toContain('design rules check passed');
    expect(r.next.map((n) => n.label)).toEqual(['Download Gerbers', 'Open Build']);
  });

  it('says why it stopped', () => {
    const r = fabResult(fab({ ok: false, drc: 'failed', downloads: [] }));
    expect(r.kind).toBe('bad');
    expect(r.detail).toBe('The design rules check found errors');
    const other = fabResult(fab({ ok: false, drc: 'not_run', output: 'reading\nerror: no footprint for C12\n' }));
    expect(other.detail).toBe('error: no footprint for C12');
  });
});

describe('buildResult', () => {
  it('counts what was written, or gives the cause', () => {
    expect(buildResult({ ok: true, written: ['a', 'b'], output: '', elapsedMs: 1200 }).title).toBe('Wrote KiCad files for 2 boards');
    const bad = buildResult({ ok: false, written: [], output: 'error: kicad.toml is not valid', elapsedMs: 10 });
    expect(bad.kind).toBe('bad');
    expect(bad.detail).toBe('error: kicad.toml is not valid');
  });
});

describe('orderResult', () => {
  it('offers the summary', () => {
    const r = orderResult({ name: 'devkit', ok: true, dir: 'fab/orders/devkit', summary: '', output: '', downloads: [{ key: 'summary', name: 'order.txt' }], elapsedMs: 9000 });
    expect(r.title).toBe('Order devkit made');
    expect(r.next[0].label).toBe('Download order.txt');
  });
});

describe('routeResult', () => {
  const report = (over: Partial<PcbReport>): PcbReport => ({
    board: 'Led',
    routed: true,
    unrouted: [],
    placementProblems: [],
    tracks: 12,
    vias: 2,
    stopped: false,
    error: null,
    elapsedMs: 4000,
    ...over,
  });
  it('says every connection is made, or how many are left', () => {
    expect(routeResult(report({})).kind).toBe('ok');
    const left = routeResult(report({ unrouted: ['a', 'b'] }));
    expect(left.kind).toBe('info');
    expect(left.title).toContain('2 connections left');
  });
  it('says a board that could not be placed, with the cause', () => {
    const r = routeResult(report({ error: 'the board has no outline' }));
    expect(r.kind).toBe('bad');
    expect(r.detail).toBe('the board has no outline');
  });
});
