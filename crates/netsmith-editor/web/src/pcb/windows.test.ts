import { describe, expect, it } from 'vitest';

import type { PcbBox, PcbPart, PcbRequirement } from '../api';
import { ghostOf, heldTip, isRequired, rangeText, requiredHere, requirementLines, windowBox, windowMove, windowOf } from './windows';

// The outline's top left corner is at (100, 50) on the sheet.
const frame = { originX: 100, originY: 50 };
const outline: PcbBox = { x: 100, y: 50, width: 40, height: 24 };

const req = (field: string, lo: number | null, hi: number | null, module = false): PcbRequirement => ({
  field,
  lo,
  hi,
  id: `${module ? 'block' : 'j1'}.place.${field}`,
  source: 'main.physical.nsm:5',
  module,
});

/** A part whose origin is 0.25 mm right of and above its body's top left corner, the body 2 by 5 mm, at (x, y) from the outline. */
function part(x: number, y: number, requirements: PcbRequirement[], over: Partial<PcbPart> = {}): PcbPart {
  const body = { x: 100 + x, y: 50 + y, width: 2, height: 5 };
  return {
    refdes: 'J1',
    path: 'j1',
    kind: 'Header',
    value: '',
    package: null,
    x: body.x + 0.25,
    y: body.y - 0.25,
    rotation: 90,
    side: 'top',
    locked: false,
    placement: 'hand',
    courtyard: body,
    courtyards: [body],
    body,
    height: null,
    pads: [],
    requirements,
    required: { x: null, y: null, rotation: null, side: null },
    requiredElsewhere: null,
    ...over,
  };
}

describe('the window of a part', () => {
  it('is the box its body may stand in, from its edges and its centre', () => {
    // As the recipe's header: its left edge 1.4 to 1.6 mm, its centre 11.9 to 12.1 mm down.
    const p = part(28, 10, [req('left', 1.4, 1.6), req('y', 11.9, 12.1)]);
    const w = windowOf(p, frame)!;
    expect(w.x.lo).toBeCloseTo(101.4);
    expect(w.x.hi).toBeCloseTo(101.6);
    // The top edge is the centre less half the height.
    expect(w.y.lo).toBeCloseTo(50 + 9.4);
    expect(w.y.hi).toBeCloseTo(50 + 9.6);
    const b = windowBox(p, w, outline);
    expect([b.x, b.width, b.y, b.height].map((v) => Math.round(v * 1e6) / 1e6)).toEqual([101.4, 2.2, 59.4, 5.2]);
  });

  it('spans the outline along an axis nothing bounds, and is none for a turn alone', () => {
    const p = part(5, 5, [req('right', null, 20)]);
    const b = windowBox(p, windowOf(p, frame)!, outline);
    expect([b.x, b.width, b.y, b.height]).toEqual([100, 20, 50, 24]);
    expect(windowOf(part(5, 5, [req('rotation', 90, 90)]), frame)).toBeNull();
    expect(windowOf(part(5, 5, []), frame)).toBeNull();
  });

  it('takes the tightest of several bounds on one axis', () => {
    const w = windowOf(part(5, 5, [req('left', 1, 10), req('x', 3, 6), req('left', 2.5, null, true)]), frame)!;
    expect(w.x.lo).toBeCloseTo(102.5);
    expect(w.x.hi).toBeCloseTo(105);
    expect(w.x.loBy?.module).toBe(true);
    expect(w.x.hiBy?.field).toBe('x');
  });
});

describe('the ghost', () => {
  it('is the body at the nearest place the window allows, while it is outside', () => {
    const p = part(28, 10, [req('left', 1.4, 1.6), req('y', 11.9, 12.1)]);
    const g = ghostOf(p, windowOf(p, frame)!)!;
    expect(g.x).toBeCloseTo(101.6);
    expect(g.y).toBeCloseTo(59.6);
    expect([g.width, g.height]).toEqual([2, 5]);
  });

  it('is not drawn for a part in its window', () => {
    const p = part(1.5, 9.5, [req('left', 1.4, 1.6), req('y', 11.9, 12.1)]);
    expect(ghostOf(p, windowOf(p, frame)!)).toBeNull();
  });
});

describe('a drag', () => {
  it('stops at the edge of the window, as the layout file keeps the place, and says what held it', () => {
    const p = part(1.5, 9.5, [req('left', 1.4, 1.6)]);
    const m = windowMove([p], frame, { x: 3, y: -2 });
    // The origin is 1.75 from the outline: the body may go 0.1 right, and freely along y.
    expect(m.delta.x).toBeCloseTo(0.1);
    expect(m.delta.y).toBe(-2);
    expect(m.held?.requirement.id).toBe('j1.place.left');
    expect(heldTip(m.held!).title).toBe('limited by j1.place.left (main.physical.nsm:5)');
    expect(windowMove([p], frame, { x: 0.05, y: 0 })).toEqual({ delta: { x: 0.05, y: 0 }, held: null });
  });

  it('rounds the limit inward to the file hundredth, so the saved place is still inside', () => {
    // The body's left edge at the origin less 0.2534: the window 1.4 to 1.6 allows origins 1.6534 to 1.8534.
    const p = part(1.5, 9.5, [req('left', 1.4, 1.6)], { x: 101.7534 });
    const m = windowMove([p], frame, { x: 5, y: 0 });
    // The origin is kept as 1.75: the last hundredth inside is 1.85.
    expect(m.delta.x).toBeCloseTo(0.1);
    const back = windowMove([p], frame, { x: -5, y: 0 });
    expect(back.delta.x).toBeCloseTo(-0.09);
  });

  it('lets a part outside its window come back toward it, never go further out', () => {
    const p = part(28, 10, [req('left', 1.4, 1.6)]);
    expect(windowMove([p], frame, { x: 2, y: 0 }).delta.x).toBe(0);
    expect(windowMove([p], frame, { x: -10, y: 0 }).delta.x).toBe(-10);
    // Not past the far edge either: the origin 28.25 may go to 1.65.
    expect(windowMove([p], frame, { x: -40, y: 0 }).delta.x).toBeCloseTo(-26.6);
  });

  it('names the requirement that took the most off the move', () => {
    const p = part(1.5, 9.5, [req('left', 1.4, 1.6), req('top', 9.45, 9.55)]);
    const m = windowMove([p], frame, { x: 3, y: 0.1 });
    expect(m.delta.y).toBeCloseTo(0.05);
    expect(m.held?.requirement.field).toBe('left');
  });

  it('does not move a part along an axis its requirements fix', () => {
    const p = part(4, 4, [req('x', 5, 5), req('y', 6.5, 6.5)]);
    expect(windowMove([p], frame, { x: 1, y: -1 }).delta).toEqual({ x: 0, y: 0 });
  });

  it('holds every part of a selection to its own window', () => {
    const a = part(1.5, 9.5, [req('left', 1.4, 1.6)]);
    const b = part(10, 0, [], { path: 'r1', refdes: 'R1' });
    expect(windowMove([b, a], frame, { x: 1, y: 0 }).delta.x).toBeCloseTo(0.1);
  });
});

describe('words and "Require here"', () => {
  it('names each requirement with its window', () => {
    expect(rangeText(req('left', 1.4, 1.6))).toBe('1.4 to 1.6 mm');
    expect(rangeText(req('x', null, 3))).toBe('at most 3 mm');
    expect(rangeText(req('rotation', 90, 90))).toBe('90 degrees');
    expect(requirementLines(part(0, 0, [req('top', 2, null, true)]))).toEqual(['block.place.top allows at least 2 mm, for every part of the module']);
  });

  it('states the centre of the body from the outline corner, the turn and the face', () => {
    const p = part(28, 10, [], { rotation: -90, side: 'bottom' });
    expect(requiredHere(p, frame)).toEqual({ x: 29, y: 12.5, rotation: 270, side: 'bottom' });
    expect(isRequired(p)).toBe(false);
    expect(isRequired({ ...p, required: { x: 29, y: 12.5, rotation: null, side: null } })).toBe(true);
  });
});
