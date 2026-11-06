import { describe, expect, it } from 'vitest';

import { heightAt, place, rulerTicks } from './rulerMarks';

describe('the ticks of the overview ruler', () => {
  it('are one for each problem, worded as the tooltip says them: the line and the message', () => {
    const ticks = rulerTicks(100, [{ line: 40, severity: 'error', message: 'no part called Resistr' }, { line: 7, severity: 'warning', message: 'unused net' }], null);
    expect(ticks.map((t) => [t.lane, t.kind, t.from, t.label])).toEqual([
      ['problem', 'warning', 7, 'line 7: unused net'],
      ['problem', 'error', 40, 'line 40: no part called Resistr'],
    ]);
  });
  it('say only the first line of a long message, and cut a very long one', () => {
    const [one] = rulerTicks(10, [{ line: 2, severity: 'error', message: 'first\nsecond' }], null);
    expect(one.label).toBe('line 2: first');
    const [long] = rulerTicks(10, [{ line: 2, severity: 'error', message: 'x'.repeat(300) }], null);
    expect(long.label.length).toBeLessThan(160);
    expect(long.label.endsWith('…')).toBe(true);
  });
  it('merge the consecutive lines of one kind of change into one tick, and keep two kinds apart', () => {
    const ticks = rulerTicks(100, [], { added: [10, 11, 12, 30], changed: [13, 14], deleted: [50] });
    expect(ticks.map((t) => [t.kind, t.from, t.to, t.label])).toEqual([
      ['added', 10, 12, 'lines 10 to 12: new since the ref'],
      ['changed', 13, 14, 'lines 13 to 14: changed since the ref'],
      ['added', 30, 30, 'line 30: new since the ref'],
      ['deleted', 50, 50, 'line 50: lines were removed here since the ref'],
    ]);
    expect(ticks.every((t) => t.lane === 'change')).toBe(true);
  });
  it('are in the order of the lines, a problem before a change on the same line', () => {
    const ticks = rulerTicks(100, [{ line: 12, severity: 'error', message: 'm' }], { added: [12], changed: [], deleted: [] });
    expect(ticks.map((t) => t.lane)).toEqual(['problem', 'change']);
  });
  it('keep a mark inside the file, whatever line it is given', () => {
    const ticks = rulerTicks(10, [{ line: 99, severity: 'error', message: 'm' }, { line: 0, severity: 'warning', message: 'w' }], { added: [500], changed: [], deleted: [] });
    for (const t of ticks) {
      expect(t.from).toBeGreaterThanOrEqual(1);
      expect(t.to).toBeLessThanOrEqual(10);
    }
  });
  it('are none for a file with nothing to mark', () => {
    expect(rulerTicks(50, [], null)).toEqual([]);
    expect(rulerTicks(50, [], { added: [], changed: [], deleted: [] })).toEqual([]);
  });
});

describe('where a tick or the thumb sits', () => {
  it('is its share of the track, and never thinner than 3 px', () => {
    expect(place(0, 1000, 4000, 400)).toEqual({ top: 0, height: 100 });
    expect(place(2000, 2020, 4000, 400)).toEqual({ top: 200, height: 3 });
  });
  it('stays inside the track at its end', () => {
    const p = place(3995, 4000, 4000, 400);
    expect(p.top + p.height).toBeLessThanOrEqual(400);
  });
  it('is nothing for an empty document', () => {
    expect(place(0, 0, 0, 400).top).toBe(0);
  });
  it('turns a press on the track into a height of the document', () => {
    expect(heightAt(100, 4000, 400)).toBe(1000);
    expect(heightAt(-20, 4000, 400)).toBe(0);
    expect(heightAt(900, 4000, 400)).toBe(4000);
    expect(heightAt(5, 4000, 0)).toBe(0);
  });
});
