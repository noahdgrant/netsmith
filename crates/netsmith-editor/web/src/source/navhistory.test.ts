import { describe, expect, it } from 'vitest';

import { EMPTY, LIMIT, canBack, canForward, forget, leaving, stepBack, stepForward, visit } from './navhistory';

const at = (line: number, file = 'src/main.nsm', column = 1) => ({ file, line, column });

describe('the history', () => {
  it('starts empty and cannot move', () => {
    expect(canBack(EMPTY)).toBe(false);
    expect(canForward(EMPTY)).toBe(false);
    expect(stepBack(EMPTY)).toBeNull();
    expect(stepForward(EMPTY)).toBeNull();
  });
  it('records where the cursor was and where it went, and goes back to the first', () => {
    let h = leaving(EMPTY, at(10));
    h = visit(h, at(40, 'std/parts/passive.nsm'));
    expect(canBack(h)).toBe(true);
    expect(canForward(h)).toBe(false);
    const b = stepBack(h)!;
    expect(b.place).toEqual(at(10));
    expect(canBack(b.history)).toBe(false);
    expect(canForward(b.history)).toBe(true);
    expect(stepForward(b.history)!.place.file).toBe('std/parts/passive.nsm');
  });
  it('treats the same line of the same file as one place, and keeps the newest column', () => {
    let h = visit(EMPTY, at(5, 'a', 1));
    h = visit(h, at(5, 'a', 9));
    expect(h.entries).toEqual([at(5, 'a', 9)]);
  });
  it('drops what was ahead when it goes somewhere new', () => {
    let h = visit(visit(visit(EMPTY, at(1)), at(2)), at(3));
    h = stepBack(stepBack(h)!.history)!.history;
    expect(h.index).toBe(0);
    h = visit(h, at(9));
    expect(h.entries.map((e) => e.line)).toEqual([1, 9]);
    expect(canForward(h)).toBe(false);
  });
  it('keeps at most LIMIT places, the newest', () => {
    let h = EMPTY;
    for (let i = 1; i <= LIMIT + 10; i++) h = visit(h, at(i));
    expect(h.entries.length).toBe(LIMIT);
    expect(h.entries[h.entries.length - 1].line).toBe(LIMIT + 10);
    expect(h.index).toBe(LIMIT - 1);
  });
  it('forgets the places of files that are gone and keeps the index on the same place', () => {
    let h = visit(visit(visit(EMPTY, at(1, 'a')), at(2, 'b')), at(3, 'c'));
    h = stepBack(h)!.history;
    const f = forget(h, (file) => file === 'a');
    expect(f.entries.map((e) => e.file)).toEqual(['b', 'c']);
    expect(f.index).toBe(0);
    expect(forget(h, () => true)).toEqual(EMPTY);
  });
});
