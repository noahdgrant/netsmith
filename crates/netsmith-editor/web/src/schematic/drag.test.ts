import { describe, expect, it } from 'vitest';

import { DRAG_THRESHOLD, DragSession, mirroredEntry, movedEntry, parseOrient, stepsOfMove, turnedEntry } from './drag';
import { SCALE } from './zoom';

/** Screen pixels in one grid step at zoom `k`. */
const per = (k: number) => k * SCALE * 1.27;

describe('a pointer’s move in grid steps', () => {
  it('is the move over the zoom and over the grid, rounded to a step', () => {
    const k = 0.5;
    expect(stepsOfMove(per(k) * 3, -per(k) * 2, k)).toEqual({ sx: 3, sy: -2 });
    expect(stepsOfMove(per(k) * 3.4, per(k) * 0.6, k)).toEqual({ sx: 3, sy: 1 });
    expect(stepsOfMove(per(k) * 0.4, -per(k) * 0.4, k)).toEqual({ sx: 0, sy: 0 });
  });
  it('is the same drawing distance at any zoom', () => {
    // 10 mm on the drawing is 7.87 steps: 8 at any zoom.
    for (const k of [0.1, 0.5, 1, 4]) expect(stepsOfMove(10 * SCALE * k, 0, k).sx).toBe(8);
  });
  it('has no negative zero', () => {
    expect(Object.is(stepsOfMove(-0.1, -0.1, 1).sx, 0)).toBe(true);
  });
});

describe('a drag', () => {
  const k = 1;
  it('is a click until the pointer has moved past the threshold', () => {
    const d = new DragSession(100, 100, k);
    expect(d.move(100 + DRAG_THRESHOLD - 1, 100, false).active).toBe(false);
    expect(d.finish()).toBeNull();
  });
  it('ends where it is dropped, snapped to the grid', () => {
    const d = new DragSession(100, 100, k);
    const s = d.move(100 + per(k) * 2.2, 100 - per(k) * 3.7, false);
    expect(s).toMatchObject({ active: true, sx: 2, sy: -4, pin: false });
    expect(d.finish()).toEqual({ sx: 2, sy: -4, pin: false });
  });
  it('keeps the modifier of the last move: it pins', () => {
    const d = new DragSession(0, 0, k);
    d.move(per(k) * 3, 0, false);
    d.move(per(k) * 3, 0, true);
    expect(d.finish()).toEqual({ sx: 3, sy: 0, pin: true });
  });
  it('is cancelled by Esc: it writes nothing, whatever the pointer does after', () => {
    const d = new DragSession(0, 0, k);
    d.move(per(k) * 5, 0, false);
    d.cancel();
    expect(d.cancelled).toBe(true);
    d.move(per(k) * 6, 0, false);
    expect(d.finish()).toBeNull();
  });
  it('writes nothing when it ends where it began', () => {
    const d = new DragSession(0, 0, k);
    d.move(per(k) * 4, 0, false);
    d.move(per(k) * 0.2, 0, false);
    expect(d.finish()).toBeNull();
  });
  it('measures with the zoom it began at', () => {
    const d = new DragSession(0, 0, 2);
    expect(d.move(per(2) * 2, 0, false).sx).toBe(2);
  });
});

describe('the entry after a move', () => {
  it('is an offset from where the engine puts the block, added to the offset the part has', () => {
    expect(movedEntry(undefined, 2, -1, false, null)).toEqual({ dx: 2.54, dy: -1.27 });
    expect(movedEntry({ dx: 2.54, dy: 0 }, 1, 1, false, null)).toEqual({ dx: 3.81, dy: 1.27 });
  });
  it('goes back to no offset when the move undoes the offset, and keeps a turn', () => {
    expect(movedEntry({ dx: 1.27, dy: 0 }, -1, 0, false, null)).toEqual({});
    expect(movedEntry({ dx: 1.27, dy: 0, rotate: 90, mirror: true }, -1, 0, false, null)).toEqual({ rotate: 90, mirror: true });
  });
  it('starts from nothing when the drawing refused the entry (it is drawn at the engine’s place)', () => {
    expect(movedEntry({ dx: 12.7, dy: 0 }, 1, 0, false, null, false)).toEqual({ dx: 1.27, dy: 0 });
  });
  it('moves a pinned part by moving its pin', () => {
    expect(movedEntry({ pin: [12.7, 0] }, 2, 1, false, null)).toEqual({ pin: [15.24, 1.27] });
    expect(movedEntry({ pin: [12.7, 0], rotate: 180 }, -1, 0, true, null)).toEqual({ pin: [11.43, 0], rotate: 180 });
  });
  it('pins the block’s origin where it is dropped, from the sheet’s frame', () => {
    // The part's origin is 8 steps right of the frame's (the reference part's origin); dropped 3 steps further right.
    const frame = { origin: [20.32, 5.08] as [number, number], frame: [10.16, 5.08] as [number, number] };
    expect(movedEntry(undefined, 3, 0, true, frame)).toEqual({ pin: [13.97, 0] });
    // Where an offset moved the reference part, the frame is where it was put by the engine (the caller subtracts the offset).
    expect(movedEntry({ dx: 1.27, dy: 0 }, 0, 2, true, frame)).toEqual({ pin: [10.16, 2.54] });
  });
  it('writes a pin on the grid whatever the origins are', () => {
    const frame = { origin: [20.1, 5.2] as [number, number], frame: [10, 5] as [number, number] };
    const e = movedEntry(undefined, 0, 0, true, frame);
    expect(e.pin).toEqual([10.16, 0]);
  });
});

describe('turning', () => {
  it('reads what the drawing says of the orientation', () => {
    expect(parseOrient('0 0')).toEqual({ mirror: false, turns: 0 });
    expect(parseOrient('1 3')).toEqual({ mirror: true, turns: 3 });
    expect(parseOrient('0 4')).toBeNull();
    expect(parseOrient(null)).toBeNull();
  });
  it('turns a quarter from the orientation drawn, not from what the file says', () => {
    // The engine turned the part a quarter by itself and the file says nothing: a turn is `rotate = 180`.
    expect(turnedEntry(undefined, { mirror: false, turns: 1 })).toEqual({ rotate: 180 });
    expect(turnedEntry({ dx: 1.27, dy: 0, rotate: 270 }, { mirror: false, turns: 3 })).toEqual({ dx: 1.27, dy: 0, rotate: 0 });
    expect(turnedEntry(undefined, { mirror: true, turns: 0 }, -1)).toEqual({ rotate: 270 });
  });
  it('mirrors from the mirror drawn', () => {
    expect(mirroredEntry(undefined, { mirror: false, turns: 2 })).toEqual({ mirror: true });
    expect(mirroredEntry({ mirror: true }, { mirror: true, turns: 0 })).toEqual({ mirror: false });
  });
});
