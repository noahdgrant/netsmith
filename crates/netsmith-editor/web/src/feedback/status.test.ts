import { describe, expect, it } from 'vitest';

import { type Row, rowOfProblem } from './problems';
import {
  ANALYSED_SHOWN_MS,
  IDLE,
  type Pending,
  analysedText,
  analysedTip,
  analysingText,
  bannerFor,
  causeOf,
  clock,
  connectionState,
  CANCELLED_TEXT,
  CANCELLING_TEXT,
  cancelledTip,
  isBusy,
  progressText,
  unitText,
  stageLines,
  step,
} from './status';

describe('clock', () => {
  it('reads seconds, then minutes', () => {
    expect(clock(0)).toBe('0 s');
    expect(clock(999)).toBe('0 s');
    expect(clock(4200)).toBe('4 s');
    expect(clock(59_900)).toBe('59 s');
    expect(clock(65_000)).toBe('1 min 5 s');
    expect(clock(46_000)).toBe('46 s');
    expect(clock(-5)).toBe('');
  });
});

describe('analysingText', () => {
  it('says nothing for a quick one, then that it is analysing, then for how long', () => {
    expect(analysingText(null)).toBeNull();
    expect(analysingText(100)).toBeNull();
    expect(analysingText(500)).toBe('Analysing…');
    expect(analysingText(1000)).toBe('Analysing… 1 s');
    expect(analysingText(46_300)).toBe('Analysing… 46 s');
  });
});

describe('step (what the page waits for)', () => {
  const run = (events: Parameters<typeof step>[1][]): Pending => events.reduce(step, IDLE);

  it('is busy from a check sent until the latest is answered', () => {
    const p = run([{ type: 'check-sent', id: 1, now: 100 }]);
    expect(isBusy(p)).toBe(true);
    expect(p.since).toBe(100);
    expect(run([{ type: 'check-sent', id: 1, now: 100 }, { type: 'checked', id: 1, latest: true }])).toEqual(IDLE);
  });

  it('keeps the clock of the first request while later ones follow', () => {
    const p = run([
      { type: 'check-sent', id: 1, now: 100 },
      { type: 'check-sent', id: 2, now: 900 },
    ]);
    expect(p.since).toBe(100);
    // The answer to the first is not the answer to the text now on the page.
    const after = step(p, { type: 'checked', id: 1, latest: false });
    expect(isBusy(after)).toBe(true);
    expect(isBusy(step(after, { type: 'checked', id: 2, latest: true }))).toBe(false);
  });

  it('waits for the rebuild after a save, and for a check typed meanwhile', () => {
    let p = run([{ type: 'saved', now: 10 }]);
    expect(isBusy(p)).toBe(true);
    p = step(p, { type: 'check-sent', id: 3, now: 20 });
    p = step(p, { type: 'rebuilt' });
    expect(isBusy(p)).toBe(true);
    p = step(p, { type: 'checked', id: 3, latest: true });
    expect(p).toEqual(IDLE);
  });

  it('gives up when the socket is lost', () => {
    expect(step(run([{ type: 'check-sent', id: 1, now: 1 }]), { type: 'lost' })).toEqual(IDLE);
  });
});

describe('connectionState', () => {
  it('is live while open', () => {
    expect(connectionState(true, true, null, 5000)).toBe('connected');
  });
  it('says connecting before the first open', () => {
    expect(connectionState(false, false, null, 0)).toBe('connecting');
  });
  it('does not flicker for a short loss', () => {
    expect(connectionState(false, true, 1000, 1800)).toBe('connected');
  });
  it('says reconnecting for a while, then offline', () => {
    expect(connectionState(false, true, 1000, 3000)).toBe('reconnecting');
    expect(connectionState(false, true, 1000, 9000)).toBe('reconnecting');
    expect(connectionState(false, true, 1000, 12_000)).toBe('offline');
  });
  it('says offline when it never connected for long', () => {
    expect(connectionState(false, false, 0, 11_000)).toBe('offline');
  });
});

describe('bannerFor', () => {
  const err = (message: string, line: number): Row =>
    rowOfProblem(
      { severity: 'error', message, rendered: '', at: { file: 'src/main.nsm', line, column: 1, endLine: line, endColumn: 2 } },
      0,
    );

  it('is nothing when the view is current', () => {
    expect(bannerFor({ stale: false, unsavedBroken: false, errors: [] })).toBeNull();
  });

  it('names the first error, its line and how many more', () => {
    const b = bannerFor({ stale: true, unsavedBroken: false, errors: [err('this `{` is never closed', 6), err('other', 9)] })!;
    expect(b.kind).toBe('stale');
    expect(b.chip).toBe('Showing last good version');
    expect(b.text).toBe('this `{` is never closed');
    expect(b.where).toBe('src/main.nsm:6');
    expect(b.more).toBe(1);
  });

  it('says so when the saved file is fine and the typed text is not', () => {
    const b = bannerFor({ stale: false, unsavedBroken: true, errors: [err('x', 2)] })!;
    expect(b.kind).toBe('unsaved');
    expect(b.detail).toContain('saved file');
  });

  it('still says something when the error has no message to show', () => {
    expect(bannerFor({ stale: true, unsavedBroken: false, errors: [] })!.text).toContain('mistake');
  });
});

describe('causeOf', () => {
  it('takes the line that says error', () => {
    expect(causeOf('reading project\nerror: no footprint for C12\n  --> a.nsm:3\n')).toBe('error: no footprint for C12');
  });
  it('takes the first line otherwise, and nothing from nothing', () => {
    expect(causeOf('\n  design rules check failed\nmore')).toBe('design rules check failed');
    expect(causeOf('')).toBe('');
    expect(causeOf(null)).toBe('');
  });
});

describe("the server's analysis", () => {
  const run = (events: Parameters<typeof step>[1][]): Pending => events.reduce(step, IDLE);
  const solve = { type: 'progress' as const, now: 1_000, stage: 'solve', board: 'Beta', done: 0, total: 1 };

  it('is busy from `analysing` to `rebuilt`, and its clock is the server\'s', () => {
    const p = run([{ type: 'analysing', since: 5_000 }]);
    expect(isBusy(p)).toBe(true);
    expect(p.since).toBe(5_000);
    expect(run([{ type: 'analysing', since: 5_000 }, { type: 'rebuilt' }])).toEqual(IDLE);
  });

  it('keeps the server\'s start over a request the page made earlier', () => {
    const p = run([{ type: 'saved', now: 9_000 }, { type: 'analysing', since: 8_500 }]);
    expect(p.since).toBe(8_500);
    // `rebuilt` ends both.
    expect(step(p, { type: 'rebuilt' })).toEqual(IDLE);
  });

  it('shows the stage with its own count and no total over all of them', () => {
    expect(progressText({ stage: 'solve', board: 'Beta', done: 0, total: 1 })).toBe('Solving Beta, 1 of 1');
    expect(progressText({ stage: 'lower', board: 'Demo/main', done: 2, total: 7 })).toBe('Lowering Demo/main, 3 of 7');
    expect(progressText({ stage: 'check', board: 'src/main.nsm', done: 0, total: 2 })).toBe('Checking src/main.nsm, 1 of 2');
    expect(progressText({ stage: 'modules', board: 'sheets/a', done: 4, total: 14 })).toBe('Checking module sheets/a, 5 of 14');
    expect(progressText({ stage: 'whatever', board: 'x', done: 0, total: 0 })).toBe('Whatever x');
    expect(analysingText(12_000, { stage: 'solve', board: 'Beta', done: 0, total: 1 })).toBe('Analysing… 12 s · Solving Beta, 1 of 1');
    expect(analysingText(100, { stage: 'solve', board: 'Beta', done: 0, total: 1 })).toBeNull();
  });

  it('remembers each stage it saw, once, with its latest total', () => {
    const p = run([
      { type: 'analysing', since: 0 },
      { ...solve, now: 1_000, stage: 'check', board: 'a.nsm', total: 1 },
      solve,
      { ...solve, now: 31_000, stage: 'lower' },
      { ...solve, now: 32_000, stage: 'lower', done: 1, total: 2 },
    ]);
    expect(p.server?.seen.map((s) => [s.stage, s.total, s.at])).toEqual([
      ['check', 1, 1_000],
      ['solve', 1, 1_000],
      ['lower', 2, 31_000],
    ]);
    expect(p.server?.progress).toMatchObject({ stage: 'lower', done: 1, total: 2 });
    // The same start again changes nothing.
    expect(step(p, { type: 'analysing', since: 0 })).toEqual(p);
  });

  it('says how long each stage took, to the end of the last', () => {
    const seen = [
      { stage: 'check', total: 1, at: 0 },
      { stage: 'solve', total: 1, at: 2_000 },
      { stage: 'modules', total: 14, at: 33_000 },
    ];
    expect(stageLines(seen, 46_000)).toEqual(['Check (1 file): 2 s', 'Solve (1 board): 31 s', 'Modules (14 modules): 13 s']);
  });

  it('says `Analysed in 46 s` for a while after one the page saw, and nothing special else', () => {
    const a = { at: 100_000, took: 46_000, stages: [], seen: true };
    expect(analysedText(a, 101_000)).toBe('Analysed in 46 s');
    expect(analysedText(a, 100_000 + ANALYSED_SHOWN_MS + 1)).toBe('Analysed');
    expect(analysedText({ ...a, took: 400 }, 101_000)).toBe('Analysed');
    expect(analysedText({ ...a, seen: false }, 101_000)).toBe('Analysed');
    expect(analysedText(null, 0)).toBe('Analysed');
    expect(analysedTip({ ...a, stages: ['Solve (1 board): 31 s'] })).toBe('Analysis is up to date. The last one took 46 s.\nSolve (1 board): 31 s');
  });

  it('forgets a lost socket\'s analysis: the next connection says it again', () => {
    expect(run([{ type: 'analysing', since: 1 }, { type: 'lost' }])).toEqual(IDLE);
  });
});

describe('what the solver has counted', () => {
  const solving = { stage: 'solve', board: 'Beta', done: 0, total: 1 };

  it('says the stage\'s own count until the stage counts anything', () => {
    expect(unitText(solving)).toBeNull();
    expect(progressText(solving)).toBe('Solving Beta, 1 of 1');
  });

  it('says how far the stage is inside the board: done of total, the unit, and no board count for one board', () => {
    const p = { ...solving, unit: 'parts chosen', unitDone: 41, unitTotal: 118 };
    expect(unitText(p)).toBe('41 of 118 parts chosen');
    expect(progressText(p)).toBe('Solving Beta: 41 of 118 parts chosen');
  });

  it('keeps the board\'s own count in brackets when there are several, and never makes up a total', () => {
    const p = { stage: 'solve', board: 'Beta', done: 1, total: 3, unit: 'parts chosen', unitDone: 7, unitTotal: 20 };
    expect(progressText(p)).toBe('Solving Beta (2 of 3): 7 of 20 parts chosen');
    const open = { stage: 'lower', board: 'Beta', done: 0, total: 1, unit: 'alternative parts re-solved', unitDone: 2 };
    expect(unitText(open)).toBe('2 alternative parts re-solved');
    expect(progressText(open)).toBe('Lowering Beta: 2 alternative parts re-solved');
  });

  it('keeps the count the server last said, and drops it when the stage changes to one that counts nothing', () => {
    let p: Pending = IDLE;
    p = step(p, { type: 'analysing', since: 1000 });
    p = step(p, { type: 'progress', now: 1100, ...solving, unit: 'parts chosen', unitDone: 3, unitTotal: 9 });
    expect(p.server?.progress).toMatchObject({ unit: 'parts chosen', unitDone: 3, unitTotal: 9 });
    p = step(p, { type: 'progress', now: 1200, stage: 'draw', board: 'Beta', done: 0, total: 1 });
    expect(p.server?.progress?.unit).toBeUndefined();
    expect(progressText(p.server!.progress!)).toBe('Drawing Beta, 1 of 1');
  });
});

describe('a cancelled analysis', () => {
  const run = (events: Parameters<typeof step>[1][]): Pending => events.reduce(step, IDLE);

  it('is busy until the server says it has ended, and says it is cancelling meanwhile', () => {
    const p = run([{ type: 'analysing', since: 1000 }, { type: 'cancelling' }]);
    expect(isBusy(p)).toBe(true);
    expect(p.cancelling).toBe(true);
    expect(CANCELLING_TEXT).toBe('Cancelling…');
  });

  it('ends with `cancelled` and no `rebuilt`: the page is idle and keeps its reading', () => {
    expect(run([{ type: 'analysing', since: 1000 }, { type: 'cancelling' }, { type: 'cancelled' }])).toEqual(IDLE);
    // A page that was not busy has nothing to cancel.
    expect(run([{ type: 'cancelling' }])).toEqual(IDLE);
  });

  it('does not stop a save or a check that waits for its own answer', () => {
    const p = run([
      { type: 'saved', now: 10 },
      { type: 'analysing', since: 20 },
      { type: 'cancelled' },
    ]);
    expect(p.server).toBeNull();
    expect(p.save).toBe(false);
    expect(isBusy(p)).toBe(false);
    const waiting = run([{ type: 'check-sent', id: 4, now: 30 }, { type: 'analysing', since: 40 }, { type: 'cancelled' }]);
    expect(waiting.check).toBe(4);
    expect(isBusy(waiting)).toBe(true);
  });

  it('says what stays on screen and what to do', () => {
    expect(CANCELLED_TEXT).toBe('Analysis cancelled');
    expect(cancelledTip(true)).toMatch(/nothing is shown yet/);
    expect(cancelledTip(false)).toMatch(/reading before it/);
    expect(cancelledTip(false)).toMatch(/Click to run it again/);
  });
});
