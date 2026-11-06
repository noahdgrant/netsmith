import { afterEach, describe, expect, it } from 'vitest';

import { beginCompareProgress, compareProgress, endCompareProgress, noteCompareProgress } from './progress';

afterEach(() => endCompareProgress());

describe('the progress of a comparison', () => {
  it('starts its clock when the page asks, and takes the stage the server says', () => {
    beginCompareProgress('main');
    expect(compareProgress.value).toMatchObject({ ref: 'main', done: 0, total: 0, message: '' });
    noteCompareProgress({ board: 'main', message: 'solving TwoBoards, 1 of 2', stage: 'solve', done: 1, total: 2 });
    expect(compareProgress.value).toMatchObject({ stage: 'solve', done: 1, total: 2, message: 'solving TwoBoards, 1 of 2' });
  });
  it('keeps the stage while a message has none, and the time it began', () => {
    beginCompareProgress('main');
    const since = compareProgress.value!.since;
    noteCompareProgress({ board: 'main', message: 'reading', stage: 'extract' });
    noteCompareProgress({ board: 'main', message: 'still reading' });
    expect(compareProgress.value).toMatchObject({ stage: 'extract', message: 'still reading', since });
  });
  it('ignores what is said of another ref, and of none asked for', () => {
    noteCompareProgress({ board: 'main', message: 'x', stage: 'solve' });
    expect(compareProgress.value).toBeNull();
    beginCompareProgress('v1.0');
    noteCompareProgress({ board: 'main', message: 'x', stage: 'solve' });
    expect(compareProgress.value?.stage).toBeUndefined();
  });
  it('ends', () => {
    beginCompareProgress('main');
    endCompareProgress();
    expect(compareProgress.value).toBeNull();
  });
});
