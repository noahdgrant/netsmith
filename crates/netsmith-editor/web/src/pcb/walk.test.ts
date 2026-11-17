import { describe, expect, it } from 'vitest';
import type { PcbRouteNet, PcbWalk } from '../api';
import {
  NOTHING_CROSSED,
  NOTHING_SHOVED,
  answerTo,
  changeLayer,
  crossedText,
  dragShove,
  drawLayered,
  headingOf,
  latestOnly,
  rerouteAsk,
  reroutedText,
  saidOf,
  shoveAnswerTo,
  shoveChanges,
  shoveKey,
  shovedText,
  viaShove,
  walkKey,
} from './walk';

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
  shove: false,
  earlier: [],
  vias: [],
});

describe('the walkaround asks', () => {
  it('names an ask by what it is for', () => {
    expect(walkKey(ask(1))).toBe(walkKey(ask(1)));
    expect(walkKey(ask(1))).not.toBe(walkKey(ask(2)));
    expect(walkKey({ ...ask(1), layer: 'bottom' })).not.toBe(walkKey(ask(1)));
    expect(walkKey({ ...ask(1), heading: { x: 1, y: 0 } })).not.toBe(walkKey(ask(1)));
    // The mode is part of it: the same place asked with and without the shove has another answer.
    expect(walkKey({ ...ask(1), shove: true })).not.toBe(walkKey(ask(1)));
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
    expect(answerTo(w, { points: [w.from, w.to], reason: null, tried: 0, crossed, shoved: NOTHING_SHOVED, stuck: null }).crossed).toEqual(crossed);
    expect(answerTo(w, { points: null, reason: 'no room', tried: 0, crossed, shoved: NOTHING_SHOVED, stuck: null }).crossed).toEqual(crossed);
    expect(answerTo(w, new Error('down')).crossed).toEqual(NOTHING_CROSSED);
  });

  it('turns what a shove moved into the changes of the same edit, and says it in words', () => {
    const shoved = {
      tracks: [
        {
          net: 'GND',
          track: 2,
          layer: 'top',
          width: 0.15,
          wasFrom: { x: 0, y: 0 },
          wasTo: { x: 0, y: 2 },
          from: { x: 0, y: 0 },
          to: { x: 0.5, y: 2 },
          wasMid: null,
          mid: null,
        },
      ],
      vias: [{ net: 'VCC', via: 1, diameter: 0.45, wasAt: { x: 3, y: 3 }, at: { x: 3.4, y: 3 } }],
    };
    expect(shoveChanges(shoved)).toEqual([
      { what: 'move_track', net: 'GND', track: 2, from: { x: 0, y: 0 }, to: { x: 0.5, y: 2 } },
      { what: 'move_via', net: 'VCC', via: 1, at: { x: 3.4, y: 3 } },
    ]);
    expect(shovedText(shoved)).toBe('1 track and 1 via');
    expect(shovedText({ tracks: [shoved.tracks[0], shoved.tracks[0]], vias: [] })).toBe('2 tracks');
    expect(shovedText(NOTHING_SHOVED)).toBeNull();
    expect(shoveChanges(NOTHING_SHOVED)).toEqual([]);
  });

  it('keeps what the server says is shoved and why not, and nothing for a failed ask', () => {
    const w = { ...ask(1), shove: true };
    const shoved = { tracks: [], vias: [{ net: 'VCC', via: 1, diameter: 0.45, wasAt: { x: 3, y: 3 }, at: { x: 3.4, y: 3 } }] };
    const got = answerTo(w, { points: [w.from, w.to], reason: null, tried: 0, crossed: NOTHING_CROSSED, shoved, stuck: null });
    expect(got.shoved).toEqual(shoved);
    const stuck = answerTo(w, { points: [w.from, w.to], reason: null, tried: 4, crossed: NOTHING_CROSSED, shoved: NOTHING_SHOVED, stuck: 'track 1 of net GND cannot be moved out of the way' });
    expect(stuck.shoved).toEqual(NOTHING_SHOVED);
    expect(stuck.stuck).toContain('track 1 of net GND');
    const failed = answerTo(w, new Error('down'));
    expect(failed.shoved).toEqual(NOTHING_SHOVED);
    expect(failed.stuck).toBeNull();
  });

  it('reads an answer: corners, or the reason there are none', () => {
    const w = ask(1);
    const found = answerTo(w, { points: [w.from, w.to], reason: null, tried: 0, crossed: NOTHING_CROSSED, shoved: NOTHING_SHOVED, stuck: null });
    expect(found).toEqual({ key: walkKey(w), points: [w.from, w.to], reason: null, crossed: NOTHING_CROSSED, shoved: NOTHING_SHOVED, stuck: null });
    const none = answerTo(w, { points: null, reason: 'no way round', tried: 12, crossed: NOTHING_CROSSED, shoved: NOTHING_SHOVED, stuck: null });
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

describe('a track on several layers, a via and a drag (issue 1009)', () => {
  const p = (x: number, y: number) => ({ x, y });
  const draft = { net: 'VCC', layer: 'top', points: [p(0, 0), p(1, 0)], earlier: [], vias: [] };

  it('puts a via at the last corner when the layer changes, and takes it off when it changes back before a corner', () => {
    const on = changeLayer(draft, 'bottom');
    expect(on).toEqual({ net: 'VCC', layer: 'bottom', points: [p(1, 0)], earlier: [{ layer: 'top', points: [p(0, 0), p(1, 0)] }], vias: [p(1, 0)] });
    expect(changeLayer(on, 'top')).toEqual(draft);
    // Before the first corner there is nothing to change at: only the layer changes.
    expect(changeLayer({ ...draft, points: [p(0, 0)] }, 'bottom')).toEqual({ ...draft, points: [p(0, 0)], layer: 'bottom' });
    expect(changeLayer(draft, 'top')).toBe(draft);
  });

  it('draws each run on its layer with a via at each change, and no via after the last corner', () => {
    const on = { ...changeLayer(draft, 'bottom'), points: [p(1, 0), p(1, 1)] };
    expect(drawLayered(on)).toEqual([
      { what: 'add_track', net: 'VCC', layer: 'top', width: null, from: p(0, 0), to: p(1, 0) },
      { what: 'add_track', net: 'VCC', layer: 'bottom', width: null, from: p(1, 0), to: p(1, 1) },
      { what: 'add_via', net: 'VCC', at: p(1, 0) },
    ]);
    // On an HDI board, the via between the two layers is the one the process makes (issue 1071).
    const blind = (a: string, b: string) => (a === 'top' && b === 'inner1' ? ['top', 'inner1'] : null);
    expect(drawLayered({ ...changeLayer(draft, 'inner1'), points: [p(1, 0), p(1, 1)] }, blind)[2]).toEqual({
      what: 'add_via',
      net: 'VCC',
      at: p(1, 0),
      layers: ['top', 'inner1'],
    });
    expect(viaShove('B', 'VCC', p(1, 2), false, ['top', 'inner1']).vias).toEqual([{ at: p(1, 2), diameter: null, layers: ['top', 'inner1'] }]);
    // Ended right after a layer change: the via would go nowhere.
    expect(drawLayered(changeLayer(draft, 'bottom'))).toEqual([{ what: 'add_track', net: 'VCC', layer: 'top', width: null, from: p(0, 0), to: p(1, 0) }]);
  });

  it('names a walk by the track before its layer change too', () => {
    const w = { ...ask(1), earlier: [{ layer: 'top', points: [p(0, 0), p(1, 0)] }], vias: [p(1, 0)] };
    expect(walkKey(w)).not.toBe(walkKey(ask(1)));
    expect(walkKey({ ...w, vias: [] })).not.toBe(walkKey(w));
  });

  it('asks a via and a drag, and keeps the answer', () => {
    const via = viaShove('B', 'VCC', p(1, 2), true);
    expect(via).toEqual({ board: 'B', net: 'VCC', tracks: [], vias: [{ at: p(1, 2), diameter: null }], moving: [], shove: true });
    expect(shoveKey(via)).not.toBe(shoveKey({ ...via, shove: false }));
    const record: PcbRouteNet = {
      net: 'GND',
      lock: false,
      tracks: [{ layer: 'bottom', width: 0.3, from: p(0, 0), mid: null, to: p(1, 1), lock: false, drawn: true }],
      vias: [{ at: p(1, 1), diameter: 0.6, drill: 0.3, lock: false, drawn: true }],
    } as unknown as PcbRouteNet;
    const drag = dragShove('B', record, [
      { what: 'move_track', net: 'GND', track: 1, from: p(0, 0), to: p(2, 1) },
      { what: 'move_via', net: 'GND', via: 1, at: p(2, 1) },
    ], false);
    expect(drag.tracks).toEqual([{ layer: 'bottom', width: 0.3, from: p(0, 0), to: p(2, 1) }]);
    expect(drag.vias).toEqual([{ at: p(2, 1), diameter: 0.6 }]);
    expect(drag.moving).toEqual([
      { net: 'GND', track: 1, via: null },
      { net: 'GND', track: null, via: 1 },
    ]);
    const room = shoveAnswerTo(via, { reason: 'no room there: it is on or too near a track of net GND', shoved: NOTHING_SHOVED, stuck: null });
    expect(room.key).toBe(shoveKey(via));
    expect(room.reason).toContain('track of net GND');
    expect(shoveAnswerTo(via, new Error('down')).reason).toBe('down');
  });

  it('moves an arc whole and asks the re-route with the way found', () => {
    const arc = { net: 'GND', track: 3, layer: 'top', width: 0.2, wasFrom: p(0, 0), wasTo: p(2, 0), from: p(0, 1), to: p(2, 1), wasMid: p(1, 1), mid: p(1, 2) };
    expect(shoveChanges({ tracks: [arc], vias: [] })).toEqual([{ what: 'move_track', net: 'GND', track: 3, from: p(0, 1), to: p(2, 1), mid: p(1, 2) }]);
    const w = { ...ask(3), track: [p(-1, 0), p(0, 0)] };
    const again = rerouteAsk(w, [p(0, 0), p(1, 1), p(3, 1)]);
    expect(again.track).toEqual([p(-1, 0), p(0, 0), p(1, 1), p(3, 1)]);
    expect(reroutedText({ nets: ['VCC'], tracks: [], vias: [], left: 0 })).toBe('routed again round it (VCC)');
    expect(reroutedText({ nets: ['VCC'], tracks: [], vias: [], left: 2 })).toBe('routed again round it, 2 connections left');
  });
});
