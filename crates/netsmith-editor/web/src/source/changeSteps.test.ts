import { describe, expect, it } from 'vitest';

import { changeHunks, stepChange } from './changeSteps';

const marks = (added: number[], changed: number[] = [], deleted: number[] = []) => ({ added, changed, deleted });

describe('the runs of changed lines', () => {
  it('are the consecutive marked lines, whatever kind of change each is', () => {
    expect(changeHunks(100, marks([10, 11], [12, 20], [21, 50]))).toEqual([
      { from: 10, to: 12 },
      { from: 20, to: 21 },
      { from: 50, to: 50 },
    ]);
  });
  it('are in the order of the file and keep a line once, however it is marked', () => {
    expect(changeHunks(100, marks([30, 5], [5, 30], [5]))).toEqual([{ from: 5, to: 5 }, { from: 30, to: 30 }]);
  });
  it('stay inside the file, and are none when nothing is marked', () => {
    expect(changeHunks(10, marks([99], [], [0]))).toEqual([{ from: 1, to: 1 }, { from: 10, to: 10 }]);
    expect(changeHunks(10, marks([]))).toEqual([]);
  });
});

describe('stepping through the changes', () => {
  const hunks = changeHunks(100, marks([10, 11, 12], [30], [50, 51]));
  it('goes to the start of the next run below the cursor', () => {
    expect(stepChange(hunks, 1, 1)).toEqual({ line: 10, wrapped: false });
    expect(stepChange(hunks, 10, 1)).toEqual({ line: 30, wrapped: false });
    expect(stepChange(hunks, 11, 1)).toEqual({ line: 30, wrapped: false });
    expect(stepChange(hunks, 30, 1)).toEqual({ line: 50, wrapped: false });
  });
  it('goes to the start of the run above the cursor, past the run the cursor is in', () => {
    expect(stepChange(hunks, 99, -1)).toEqual({ line: 50, wrapped: false });
    expect(stepChange(hunks, 51, -1)).toEqual({ line: 30, wrapped: false });
    // inside the first run there is none above it: the last run, wrapped
    expect(stepChange(hunks, 12, -1)).toEqual({ line: 50, wrapped: true });
    expect(stepChange(hunks, 31, -1)).toEqual({ line: 30, wrapped: false });
  });
  it('wraps round the ends of the file and says so', () => {
    expect(stepChange(hunks, 60, 1)).toEqual({ line: 10, wrapped: true });
    expect(stepChange(hunks, 5, -1)).toEqual({ line: 50, wrapped: true });
  });
  it('stays on the only run, wrapped, when there is one', () => {
    const one = changeHunks(100, marks([40]));
    expect(stepChange(one, 40, 1)).toEqual({ line: 40, wrapped: true });
    expect(stepChange(one, 40, -1)).toEqual({ line: 40, wrapped: true });
  });
  it('is nothing for a file without changes', () => {
    expect(stepChange([], 3, 1)).toBeNull();
    expect(stepChange([], 3, -1)).toBeNull();
  });
});
