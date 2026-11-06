import { beforeEach, describe, expect, it } from 'vitest';

import { AUTO_OVER, barText, collapsedRegions, hideChoice, hidesUnchanged, openBar, openedBars, regionAt, resetBars } from './collapse';
import { changeStarts, inlineRows, sideRows, splitLines } from './linediff';

const flags = (s: string) => [...s].map((c) => c === 'x');

describe('the runs of unchanged lines that are hidden', () => {
  it('leave 3 lines of context on each side of a change', () => {
    // 20 rows, a change at row 10
    const rows = flags('..........x.........');
    expect(collapsedRegions(rows)).toEqual([
      { from: 0, to: 7 },
      { from: 14, to: 20 },
    ]);
  });
  it('hide the whole stretch between two changes that are far apart, and none that are near', () => {
    expect(collapsedRegions(flags('x..........x'))).toEqual([{ from: 4, to: 8 }]);
    // 6 unchanged rows between: exactly the context of both, nothing to hide
    expect(collapsedRegions(flags('x......x'))).toEqual([]);
    // 7 between: one row would hide, fewer than the minimum
    expect(collapsedRegions(flags('x.......x'))).toEqual([]);
  });
  it('hide a run only when that saves at least 3 lines', () => {
    expect(collapsedRegions(flags('x........x'))).toEqual([]);
    expect(collapsedRegions(flags('x.........x'))).toEqual([{ from: 4, to: 7 }]);
    expect(collapsedRegions(flags('x..........x'))).toEqual([{ from: 4, to: 8 }]);
    expect(collapsedRegions(flags('...x'))).toEqual([]);
    expect(collapsedRegions(flags('......x'))).toEqual([{ from: 0, to: 3 }]);
  });
  it('hide nothing when nothing changed, and nothing in an empty file', () => {
    expect(collapsedRegions(flags('..........'))).toEqual([]);
    expect(collapsedRegions([])).toEqual([]);
  });
  it('never hide a changed row', () => {
    for (const s of ['x.........x.........x', '.x.......................x.', 'xxxxxxxx', '..........xxx..........']) {
      const rows = flags(s);
      for (const r of collapsedRegions(rows)) for (let i = r.from; i < r.to; i++) expect(rows[i], `${s} row ${i}`).toBe(false);
    }
  });
  it('keep the context size the caller asks for', () => {
    expect(collapsedRegions(flags('..........x..........'), 0)).toEqual([
      { from: 0, to: 10 },
      { from: 11, to: 21 },
    ]);
  });
});

describe('with the rows of a real diff', () => {
  const lines = (n: number, edit: Record<number, string> = {}) => Array.from({ length: n }, (_, i) => edit[i] ?? `line ${i}`);
  it('hide the unchanged lines of a long file in the inline view and in the side view the same way', () => {
    const a = lines(100);
    const b = lines(100, { 50: 'line 50 changed' });
    const inline = inlineRows(a, b);
    const sides = sideRows(a, b);
    const ri = collapsedRegions(inline.map((r) => r.kind !== 'equal'));
    const rs = collapsedRegions(sides.map((r) => !(r.left && r.right && r.left.kind === 'equal')));
    // each shows the lines 47-49 before the change and 52-54 after (0-based 47..49 and 51..53 in `a`)
    expect(ri).toEqual([
      { from: 0, to: 47 },
      { from: 55, to: 101 },
    ]);
    expect(rs).toEqual([
      { from: 0, to: 47 },
      { from: 54, to: 100 },
    ]);
    expect(inline[ri[0].to].aLine).toBe(48);
    expect(inline[ri[1].from - 4].kind).toBe('add');
  });
  it('leave every step of the changes (F7) outside the hidden runs', () => {
    const a = lines(200);
    const b = lines(200, { 10: 'x', 90: 'y', 91: 'z', 199: 'w' });
    const rows = inlineRows(a, b);
    const regions = collapsedRegions(rows.map((r) => r.kind !== 'equal'));
    expect(regions.length).toBeGreaterThan(1);
    for (const at of changeStarts(rows)) expect(regionAt(regions, at)).toBeUndefined();
  });
  it('count the lines a bar hides, in words', () => {
    const rows = inlineRows(lines(100), lines(100, { 50: 'x' }));
    const [first] = collapsedRegions(rows.map((r) => r.kind !== 'equal'));
    expect(barText(first.to - first.from)).toBe('47 unchanged lines');
    expect(barText(1)).toBe('1 unchanged line');
  });
  it('have equal rows that carry a base line number to name a bar by', () => {
    const rows = inlineRows(lines(30), lines(30, { 20: 'x' }));
    const [first] = collapsedRegions(rows.map((r) => r.kind !== 'equal'));
    expect(rows[first.from].aLine).toBe(1);
    expect(splitLines('a\nb\n')).toEqual(['a', 'b']);
  });
});

describe('whether a file hides its unchanged lines', () => {
  it('is the person\'s choice when there is one', () => {
    expect(hidesUnchanged(true, 5)).toBe(true);
    expect(hidesUnchanged(false, 5000)).toBe(false);
  });
  it('is on for a file over 60 lines and off otherwise when no choice was made', () => {
    expect(hidesUnchanged(null, AUTO_OVER)).toBe(false);
    expect(hidesUnchanged(null, AUTO_OVER + 1)).toBe(true);
    expect(hidesUnchanged(null, 0)).toBe(false);
  });
});

describe('what the page remembers', () => {
  beforeEach(() => {
    resetBars();
    hideChoice.value = null;
  });
  it('is the bars opened in each file', () => {
    expect([...openedBars('a.nsm')]).toEqual([]);
    openBar('a.nsm', 1);
    openBar('a.nsm', 60);
    openBar('b.nsm', 7);
    expect([...openedBars('a.nsm')]).toEqual([1, 60]);
    expect([...openedBars('b.nsm')]).toEqual([7]);
    resetBars('a.nsm');
    expect([...openedBars('a.nsm')]).toEqual([]);
    expect([...openedBars('b.nsm')]).toEqual([7]);
  });
  it('hands out a new set when a bar opens (a view that holds the old one does not change under it)', () => {
    const before = openedBars('a.nsm');
    openBar('a.nsm', 3);
    expect(before.has(3)).toBe(false);
    expect(openedBars('a.nsm').has(3)).toBe(true);
  });
  it('is the choice of the switch', () => {
    expect(hideChoice.value).toBeNull();
    hideChoice.value = false;
    expect(hidesUnchanged(hideChoice.value, 500)).toBe(false);
  });
});
