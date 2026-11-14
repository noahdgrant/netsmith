import { describe, expect, it } from 'vitest';
import type { PcbWalk } from '../api';
import { NOTHING_CROSSED, answerTo, crossedText, headingOf, saidOf, latestOnly, walkKey } from './walk';

/** Let the promises that are settled run their continuations. */
const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

const ask = (x: number): PcbWalk => ({
  board: 'B',
  net: 'VCC',
  layer: 'top',
  width: null,
  from: { x: 0, y: 0 },
  to: { x, y: 1 },
  heading: null,
  track: [],
});

describe('the walkaround asks', () => {
  it('names an ask by what it is for', () => {
    expect(walkKey(ask(1))).toBe(walkKey(ask(1)));
    expect(walkKey(ask(1))).not.toBe(walkKey(ask(2)));
    expect(walkKey({ ...ask(1), layer: 'bottom' })).not.toBe(walkKey(ask(1)));
    expect(walkKey({ ...ask(1), heading: { x: 1, y: 0 } })).not.toBe(walkKey(ask(1)));
  });

  it('heads the way the last segment of the draft arrives, leaving out a segment of no length', () => {
    expect(headingOf([{ x: 1, y: 1 }])).toBeNull();
    expect(
      headingOf([
        { x: 0, y: 0 },
        { x: 2, y: 1 },
      ]),
    ).toEqual({ x: 2, y: 1 });
    expect(
      headingOf([
        { x: 0, y: 0 },
        { x: 2, y: 1 },
        { x: 2, y: 1 },
      ]),
    ).toEqual({ x: 2, y: 1 });
  });

  it('names the ask by the track so far too: the router copper the whole track crosses depends on it', () => {
    const [a, b] = [{ x: 0, y: 0 }, { x: 1, y: 0 }];
    expect(walkKey({ ...ask(1), track: [a] })).not.toBe(walkKey(ask(1)));
    expect(walkKey({ ...ask(1), track: [a, b] })).not.toBe(walkKey({ ...ask(1), track: [a] }));
  });

  it('says in words how much of the router\'s copper is crossed, or nothing when none is', () => {
    const track = { net: 'GND', layer: 'top', width: 0.2, from: { x: 0, y: 0 }, mid: null, to: { x: 1, y: 0 } };
    const via = { net: 'GND', at: { x: 0, y: 0 }, diameter: 0.45 };
    expect(crossedText(NOTHING_CROSSED)).toBeNull();
    expect(crossedText({ tracks: [track], vias: [] })).toBe('1 router track');
    expect(crossedText({ tracks: [track, track], vias: [via] })).toBe('2 router tracks and 1 via');
    expect(crossedText({ tracks: [], vias: [via, via] })).toBe('2 vias');
  });

  it('tells the person the router\'s copper was discarded when the board was routed', () => {
    expect(saidOf('added a track to net GND', false)).toBe('added a track to net GND');
    expect(saidOf('added a track to net GND', true)).toBe(
      "added a track to net GND; the router's copper was discarded: route again to lay it round the new copper",
    );
  });

  it('keeps what the server says is crossed, and none for a failed ask', () => {
    const w = ask(1);
    const track = { net: 'GND', layer: 'top', width: 0.2, from: { x: 0, y: 0 }, mid: null, to: { x: 1, y: 0 } };
    const crossed = { tracks: [track], vias: [] };
    expect(answerTo(w, { points: [w.from, w.to], reason: null, tried: 0, crossed }).crossed).toEqual(crossed);
    expect(answerTo(w, { points: null, reason: 'no room', tried: 0, crossed }).crossed).toEqual(crossed);
    expect(answerTo(w, new Error('down')).crossed).toEqual(NOTHING_CROSSED);
  });

  it('reads an answer: corners, or the reason there are none', () => {
    const w = ask(1);
    const found = answerTo(w, { points: [w.from, w.to], reason: null, tried: 0, crossed: NOTHING_CROSSED });
    expect(found).toEqual({ key: walkKey(w), points: [w.from, w.to], reason: null, crossed: NOTHING_CROSSED });
    const none = answerTo(w, { points: null, reason: 'no way round', tried: 12, crossed: NOTHING_CROSSED });
    expect(none.points).toBeNull();
    expect(none.reason).toBe('no way round');
    // A failed request is a reason too, so the click is refused with it.
    expect(answerTo(w, new Error('offline')).reason).toBe('offline');
  });

  it('runs one ask at a time and, of those that come while it runs, only the last', async () => {
    const ran: number[] = [];
    const answered: number[] = [];
    const release: (() => void)[] = [];
    const walk = latestOnly(
      (n: number) =>
        new Promise<number>((resolve) => {
          ran.push(n);
          release.push(() => resolve(n * 10));
        }),
      (_, r) => answered.push(r as number),
    );
    walk(1);
    walk(2);
    walk(3);
    // One runs; the two that came meanwhile are one: the newest.
    expect(ran).toEqual([1]);
    release[0]();
    await flush();
    expect(ran).toEqual([1, 3]);
    release[1]();
    await flush();
    expect(answered).toEqual([10, 30]);
    // Idle again: the next ask runs at once.
    walk(4);
    expect(ran).toEqual([1, 3, 4]);
  });

  it('gives a failed run to done as an error and goes on', async () => {
    const got: unknown[] = [];
    const walk = latestOnly(
      async (n: number) => {
        if (n === 1) throw new Error('down');
        return n;
      },
      (_, r) => got.push(r instanceof Error ? r.message : r),
    );
    walk(1);
    await flush();
    walk(2);
    await flush();
    expect(got).toEqual(['down', 2]);
  });
});
