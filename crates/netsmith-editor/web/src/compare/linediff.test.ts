import { describe, expect, it } from 'vitest';

import { changeStarts, diffLines, gutterMarks, inlineRows, intraLine, sideRows, splitLines, tokens } from './linediff';

const lines = (s: string) => splitLines(s);
/** Apply a diff to `a` and see `b`: the script must be a correct one, not only a short one. */
function replay(a: string[], b: string[]): string[] {
  const out: string[] = [];
  for (const h of diffLines(a, b)) {
    if (h.kind === 'equal') {
      expect(a.slice(h.aFrom, h.aTo)).toEqual(b.slice(h.bFrom, h.bTo));
      out.push(...b.slice(h.bFrom, h.bTo));
    } else out.push(...b.slice(h.bFrom, h.bTo));
  }
  return out;
}

describe('splitLines', () => {
  it('counts lines as an editor does', () => {
    expect(splitLines('')).toEqual([]);
    expect(splitLines('a\n')).toEqual(['a']);
    expect(splitLines('a\nb')).toEqual(['a', 'b']);
    expect(splitLines('a\n\n')).toEqual(['a', '']);
    expect(splitLines('a\r\nb\r\n')).toEqual(['a', 'b']);
  });
});

describe('diffLines', () => {
  it('is one equal run for the same text, and nothing for two empty ones', () => {
    expect(diffLines(lines('a\nb\n'), lines('a\nb\n'))).toEqual([{ kind: 'equal', aFrom: 0, aTo: 2, bFrom: 0, bTo: 2 }]);
    expect(diffLines([], [])).toEqual([]);
  });
  it('sees an insert, a delete and a replacement', () => {
    expect(diffLines(['a', 'c'], ['a', 'b', 'c']).map((h) => h.kind)).toEqual(['equal', 'insert', 'equal']);
    expect(diffLines(['a', 'b', 'c'], ['a', 'c']).map((h) => h.kind)).toEqual(['equal', 'delete', 'equal']);
    const r = diffLines(['a', 'b', 'c'], ['a', 'X', 'c']);
    expect(r.map((h) => h.kind)).toEqual(['equal', 'replace', 'equal']);
    expect(r[1]).toMatchObject({ aFrom: 1, aTo: 2, bFrom: 1, bTo: 2 });
  });
  it('a whole file added, and a whole file removed', () => {
    expect(diffLines([], ['a', 'b'])).toEqual([{ kind: 'insert', aFrom: 0, aTo: 0, bFrom: 0, bTo: 2 }]);
    expect(diffLines(['a', 'b'], [])).toEqual([{ kind: 'delete', aFrom: 0, aTo: 2, bFrom: 0, bTo: 0 }]);
  });
  it('finds the shortest script where the lines move about', () => {
    const a = 'a b c d e f g'.split(' ');
    const b = 'a c d x e f y g'.split(' ');
    const runs = diffLines(a, b);
    expect(replay(a, b)).toEqual(b);
    const edits = runs.filter((h) => h.kind !== 'equal');
    // b is a with `b` gone and `x` and `y` added: three edits
    expect(edits.reduce((n, h) => n + (h.aTo - h.aFrom) + (h.bTo - h.bFrom), 0)).toBe(3);
  });
  it('replays correctly on many random edits', () => {
    let seed = 7;
    const rnd = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
    for (let round = 0; round < 60; round++) {
      const a = Array.from({ length: 40 }, (_, i) => `line ${i % 9}`);
      const b = a.filter(() => rnd() > 0.15);
      for (let k = 0; k < 6; k++) b.splice(Math.floor(rnd() * b.length), 0, `new ${Math.floor(rnd() * 5)}`);
      expect(replay(a, b)).toEqual(b);
      const covered = diffLines(a, b).reduce((n, h) => n + (h.aTo - h.aFrom), 0);
      expect(covered).toBe(a.length);
    }
  });
  it('does not stall on a file that was rewritten', () => {
    const a = Array.from({ length: 6000 }, (_, i) => `old ${i}`);
    const b = Array.from({ length: 6000 }, (_, i) => `new ${i}`);
    const t = Date.now();
    const runs = diffLines(a, b);
    expect(Date.now() - t).toBeLessThan(3000);
    expect(replay(a, b)).toEqual(b);
    expect(runs).toHaveLength(1);
    expect(runs[0].kind).toBe('replace');
  });
});

describe('gutterMarks', () => {
  it('marks new lines, changed lines and where lines are gone', () => {
    const base = ['a', 'b', 'c', 'd', 'e'];
    const head = ['a', 'B', 'c', 'x', 'y', 'e'];
    // b became B (changed); d became x and y (one changed, one new)
    expect(gutterMarks(base, head)).toEqual({ added: [5], changed: [2, 4], deletedAfter: [], hunks: 2, lines: 3 });
    expect(gutterMarks(['a', 'b', 'c'], ['a', 'c'])).toEqual({ added: [], changed: [], deletedAfter: [1], hunks: 1, lines: 1 });
    expect(gutterMarks(['a', 'b'], ['b'])).toEqual({ added: [], changed: [], deletedAfter: [0], hunks: 1, lines: 1 });
  });
  it('has nothing for a file that is the same, and everything for a new one', () => {
    expect(gutterMarks(['a'], ['a'])).toEqual({ added: [], changed: [], deletedAfter: [], hunks: 0, lines: 0 });
    expect(gutterMarks([], ['a', 'b'])).toEqual({ added: [1, 2], changed: [], deletedAfter: [], hunks: 1, lines: 2 });
  });
  it('says where fewer lines replace more', () => {
    expect(gutterMarks(['a', 'b', 'c', 'd'], ['a', 'X', 'd'])).toEqual({ added: [], changed: [2], deletedAfter: [2], hunks: 1, lines: 2 });
  });
});

describe('inside a line', () => {
  it('splits into words, spaces and marks', () => {
    expect(tokens('.r1: Resistor { .resistance &= 150ohm }')).toEqual(['.', 'r1', ':', ' ', 'Resistor', ' ', '{', ' ', '.', 'resistance', ' ', '&', '=', ' ', '150ohm', ' ', '}']);
  });
  it('marks only what changed', () => {
    const a = '    .resistance &= 100ohm +/- 1%';
    const b = '    .resistance &= 150ohm +/- 1%';
    const m = intraLine(a, b);
    expect(m.a.map(([x, y]) => a.slice(x, y))).toEqual(['100ohm']);
    expect(m.b.map(([x, y]) => b.slice(x, y))).toEqual(['150ohm']);
  });
  it('marks nothing in a line that is mostly new', () => {
    expect(intraLine('abc', 'xyz qrs tuv')).toEqual({ a: [], b: [] });
  });
});

describe('rows of a view', () => {
  const base = lines('a\nb\nc\nd\n');
  const head = lines('a\nB\nc\nd\ne\n');
  it('inline: the removed line, then the added one, with both numbers', () => {
    const rows = inlineRows(base, head);
    expect(rows.map((r) => `${r.kind}:${r.text}`)).toEqual(['equal:a', 'del:b', 'add:B', 'equal:c', 'equal:d', 'add:e']);
    expect(rows[1].aLine).toBe(2);
    expect(rows[1].bLine).toBeUndefined();
    expect(rows[2].aLine).toBeUndefined();
    expect(rows[2].bLine).toBe(2);
    expect(rows[0]).toMatchObject({ aLine: 1, bLine: 1 });
  });
  it('side by side: a gap where a side has no line, so the rows line up', () => {
    const rows = sideRows(base, head);
    expect(rows).toHaveLength(5);
    expect(rows[1].left?.text).toBe('b');
    expect(rows[1].right?.text).toBe('B');
    expect(rows[4].left).toBeNull();
    expect(rows[4].right?.text).toBe('e');
  });
  it('finds where each change starts', () => {
    expect(changeStarts(inlineRows(base, head))).toEqual([1, 5]);
    expect(changeStarts(sideRows(base, head))).toEqual([1, 4]);
    expect(changeStarts(inlineRows(base, base))).toEqual([]);
  });
});
