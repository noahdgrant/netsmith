import { describe, expect, it } from 'vitest';
import type { PcbPart } from '../api';
import {
  NO_HISTORY,
  drawKeepout,
  flipParts,
  invert,
  keepoutEntry,
  lockParts,
  moveParts,
  placementOf,
  record,
  redoing,
  releaseParts,
  rotateParts,
  undoing,
} from './edits';

const frame = { originX: 100, originY: 50 };

function part(path: string, x: number, y: number, over: Partial<PcbPart> = {}): PcbPart {
  return {
    refdes: path.toUpperCase(),
    path,
    kind: 'Resistor',
    value: '10k',
    package: '0402',
    x: 100 + x,
    y: 50 + y,
    rotation: 0,
    side: 'top',
    locked: false,
    placement: 'derived',
    courtyard: { x: 100 + x - 1, y: 50 + y - 1, width: 2, height: 2 },
    body: { x: 100 + x - 1, y: 50 + y - 1, width: 2, height: 2 },
    height: null,
    pads: [],
    ...over,
  };
}

describe('placements are in the layout file coordinates', () => {
  it('counts from the outline corner and drops float dust', () => {
    const p = placementOf(part('r1', 7.25, 5.5), frame);
    expect(p).toMatchObject({ x: 7.25, y: 5.5, rotation: 0, side: 'top', locked: false, source: null });
    expect(placementOf(part('r1', 0.1 + 0.2, 1), frame).x).toBe(0.3);
  });
});

describe('moving', () => {
  it('writes the new place and says who did it; before is no entry for a derived part', () => {
    const [c] = moveParts([part('r1', 10, 10)], frame, { x: 2, y: -1 });
    expect(c).toMatchObject({ what: 'part', path: 'r1', before: null });
    expect(c.what === 'part' && c.after).toMatchObject({ x: 12, y: 9, source: 'hand' });
  });

  it('has the old entry as before when the file has one', () => {
    const p = part('r1', 10, 10, { placement: 'hand', rotation: 90 });
    const [c] = moveParts([p], frame, { x: 1, y: 0 });
    expect(c.what === 'part' && c.before).toMatchObject({ x: 10, y: 10, rotation: 90, source: 'hand' });
    expect(c.what === 'part' && c.after).toMatchObject({ x: 11, rotation: 90 });
  });

  it('leaves a locked part where it is, and a move by nothing is no edit', () => {
    expect(moveParts([part('r1', 1, 1, { locked: true })], frame, { x: 3, y: 3 })).toEqual([]);
    expect(moveParts([part('r1', 1, 1)], frame, { x: 0, y: 0 })).toEqual([]);
  });
});

describe('rotating', () => {
  it('turns one part on its own origin', () => {
    const [c] = rotateParts([part('r1', 10, 10, { rotation: 270 })], frame, 90);
    expect(c.what === 'part' && c.after).toMatchObject({ x: 10, y: 10, rotation: 0 });
  });

  it('turns a selection about its middle, counter-clockwise on screen', () => {
    // Two parts side by side, the middle between them at x = 10.
    const [a, b] = rotateParts([part('a', 8, 10), part('b', 12, 10)], frame, 90);
    const at = (c: typeof a) => (c.what === 'part' ? c.after! : (null as never));
    expect(at(a).x).toBeCloseTo(10);
    expect(at(a).y).toBeCloseTo(12);
    expect(at(b).x).toBeCloseTo(10);
    expect(at(b).y).toBeCloseTo(8);
    expect([at(a).rotation, at(b).rotation]).toEqual([90, 90]);
  });

  it('skips what is locked', () => {
    expect(rotateParts([part('a', 1, 1, { locked: true })], frame, 90)).toEqual([]);
  });
});

describe('flipping', () => {
  it('puts a part on the other face and negates its turn', () => {
    const [c] = flipParts([part('r1', 10, 10, { rotation: 90 })], frame);
    expect(c.what === 'part' && c.after).toMatchObject({ x: 10, side: 'bottom', rotation: 270 });
    const [back] = flipParts([part('r1', 10, 10, { side: 'bottom', rotation: 270 })], frame);
    expect(back.what === 'part' && back.after).toMatchObject({ side: 'top', rotation: 90 });
  });

  it('mirrors a selection about its middle', () => {
    const [a, b] = flipParts([part('a', 8, 10), part('b', 14, 10)], frame);
    expect(a.what === 'part' && a.after?.x).toBe(14);
    expect(b.what === 'part' && b.after?.x).toBe(8);
  });
});

describe('locking and releasing', () => {
  it('locks where the part is and skips what is already locked', () => {
    const cs = lockParts([part('a', 3, 4), part('b', 1, 1, { locked: true })], frame, true);
    expect(cs).toHaveLength(1);
    expect(cs[0].what === 'part' && cs[0].after).toMatchObject({ x: 3, y: 4, locked: true });
  });

  it('hands a part back to the placer by removing its entry, unless it is locked', () => {
    const cs = releaseParts(
      [part('a', 3, 4, { placement: 'hand' }), part('b', 1, 1), part('c', 2, 2, { placement: 'hand', locked: true })],
      frame,
    );
    expect(cs.map((c) => c.what === 'part' && c.path)).toEqual(['a']);
    expect(cs[0].what === 'part' && cs[0].after).toBeNull();
  });
});

describe('keepouts', () => {
  it('draws one from a box on the sheet, in the file coordinates', () => {
    const c = drawKeepout('k1', { x: 110, y: 60, width: 5, height: 2.5 }, frame, {
      layers: ['top'],
      tracks: true,
      vias: false,
      copper: true,
      parts: false,
    });
    expect(c).toMatchObject({ what: 'keepout', name: 'k1', before: null });
    expect(c.what === 'keepout' && c.after).toMatchObject({ x: 10, y: 10, drawn: { width: 5, height: 2.5, layers: ['top'] } });
  });
});

describe('what the file has of a keepout', () => {
  const k = (origin: string, area: { x: number; y: number; width: number; height: number } | null) => ({
    area,
    layers: ['top'],
    tracks: true,
    vias: false,
    copper: true,
    parts: true,
    origin,
  });

  it('is a place for one the source sizes and the whole of one that was drawn', () => {
    const area = { x: 118, y: 52, width: 14, height: 5 };
    expect(keepoutEntry(k('placed', area), frame)).toEqual({ x: 18, y: 2, drawn: null });
    expect(keepoutEntry(k('drawn', area), frame)).toEqual({
      x: 18,
      y: 2,
      drawn: { width: 14, height: 5, layers: ['top'], tracks: true, vias: false, copper: true, parts: true },
    });
  });

  it('is nothing for one the editor does not change, or one that has no place', () => {
    const area = { x: 118, y: 52, width: 14, height: 5 };
    expect(keepoutEntry(k('source', area), frame)).toBeNull();
    expect(keepoutEntry(k('part', area), frame)).toBeNull();
    expect(keepoutEntry(k('placed', null), frame)).toBeNull();
  });
});

describe('history', () => {
  const step = (label: string) => ({ label, changes: moveParts([part(label, 1, 1)], frame, { x: 1, y: 0 }) });

  it('undoes the last step with its inverse and can redo it', () => {
    let h = record(NO_HISTORY, step('a'));
    h = record(h, step('b'));
    const u = undoing(h)!;
    expect(u.step.label).toBe('b');
    const [done] = u.step.changes;
    const [run] = u.run;
    expect(run).toMatchObject({ path: 'b', before: done.what === 'part' ? done.after : null, after: null });
    expect(u.next.undo.map((s) => s.label)).toEqual(['a']);
    const r = redoing(u.next)!;
    expect(r.step.label).toBe('b');
    expect(r.run).toEqual(u.step.changes);
    expect(r.next.redo).toEqual([]);
  });

  it('forgets what could be redone when something new is done', () => {
    const u = undoing(record(NO_HISTORY, step('a')))!;
    expect(u.next.redo).toHaveLength(1);
    expect(record(u.next, step('c')).redo).toEqual([]);
  });

  it('has nothing to undo or redo at the start', () => {
    expect(undoing(NO_HISTORY)).toBeNull();
    expect(redoing(NO_HISTORY)).toBeNull();
  });

  it('inverts a group of changes in the opposite order', () => {
    const cs = [...moveParts([part('a', 1, 1)], frame, { x: 1, y: 0 }), ...moveParts([part('b', 1, 1)], frame, { x: 1, y: 0 })];
    expect(invert(cs).map((c) => c.what === 'part' && c.path)).toEqual(['b', 'a']);
    expect(invert(invert(cs))).toEqual(cs);
  });

  it('is bounded', () => {
    let h = NO_HISTORY;
    for (let i = 0; i < 250; i++) h = record(h, step(`s${i}`));
    expect(h.undo).toHaveLength(200);
    expect(h.undo[199].label).toBe('s249');
  });
});
