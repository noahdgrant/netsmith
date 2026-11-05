import { describe, expect, it } from 'vitest';

import { type Row, rowOfProblem } from './problems';
import {
  IDLE,
  type Pending,
  analysingText,
  bannerFor,
  causeOf,
  clock,
  connectionState,
  isBusy,
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
