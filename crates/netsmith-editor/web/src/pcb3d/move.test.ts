import { describe, expect, it } from 'vitest';

import type { PcbBox, PcbKeepout, PcbPart, PcbScene } from '../api';
import { editKey, moveTo, startMove, toSceneOffset } from './move';

const box = (x: number, y: number, width: number, height: number): PcbBox => ({ x, y, width, height });

function part(path: string, body: PcbBox, height: number | null, locked = false): PcbPart {
  return {
    refdes: path.toUpperCase(),
    path,
    kind: 'Header',
    value: '',
    package: null,
    x: body.x,
    y: body.y,
    rotation: 0,
    side: 'top',
    locked,
    placement: 'hand',
    courtyard: body,
    courtyards: [body],
    body,
    height,
    pads: [],
  };
}

// A lid over x from 0 to 20 that allows 3 mm on the top face; the sheet's frame starts at (0, 0).
const lid: PcbKeepout = { name: 'lid', origin: 'source', area: box(0, 0, 20, 24), width: 20, height: 24, layers: ['top'], tracks: false, vias: false, copper: false, parts: false, maxHeight: 3 };

const sceneOf = (parts: PcbPart[], keepouts: PcbKeepout[] = []) => ({ parts, keepouts, originX: 0, originY: 0 }) as unknown as PcbScene;

describe('a drag in 3D', () => {
  it('starts only on a picked part', () => {
    const sc = sceneOf([part('a', box(2, 2, 2, 2), 1), part('b', box(8, 2, 2, 2), 1)]);
    expect(startMove(sc, ['a'], 'b', { x: 0, y: 0 })).toBeNull();
    expect(startMove(sc, [], 'a', { x: 0, y: 0 })).toBeNull();
    expect(startMove(sc, ['a', 'b'], 'b', { x: 0, y: 0 })?.free.map((p) => p.path)).toEqual(['a', 'b']);
  });

  it('moves on the sheet as the pointer moves in the plane, with the y turned (the scene grows up the board)', () => {
    const sc = sceneOf([part('a', box(2.3, 2, 2, 2), 1)]);
    const m = startMove(sc, ['a'], 'a', { x: 10, y: 10 })!;
    // 3 mm right and 2 mm toward the board's top edge in the scene: right and up on the sheet.
    expect(moveTo(m, { x: 13, y: 12 }, 0).delta).toEqual({ x: 3, y: -2 });
    // On a 1 mm grid the part taken ends on the grid: 2.3 + 3.4 = 5.7 goes to 6.
    expect(moveTo(m, { x: 13.4, y: 10 }, 1).delta.x).toBeCloseTo(3.7, 9);
    expect(toSceneOffset({ x: 3, y: -2 })).toEqual({ x: 3, y: 2 });
  });

  it('leaves the locked parts where they are', () => {
    const sc = sceneOf([part('a', box(2, 2, 2, 2), 1, true), part('b', box(8, 2, 2, 2), 1)]);
    const m = startMove(sc, ['a', 'b'], 'a', { x: 0, y: 0 })!;
    expect(m.free.map((p) => p.path)).toEqual(['b']);
    expect(m.parts.length).toBe(2);
    // The part taken is locked: the free one ends on the grid instead.
    expect(m.anchor).toEqual({ x: 8, y: 2 });
  });

  it('stops a part that is too tall at the edge of a height limit, as the 2D drag does', () => {
    const sc = sceneOf([part('tall', box(25, 5, 4, 4), 9)], [lid]);
    const m = startMove(sc, ['tall'], 'tall', { x: 0, y: 0 })!;
    const r = moveTo(m, { x: -10, y: 0 }, 1);
    expect(r.delta).toEqual({ x: -5, y: 0 });
    expect(r.held?.zone).toBe('lid');
    // One that fits goes under it.
    const flat = startMove(sceneOf([part('flat', box(25, 5, 4, 4), 1.5)], [lid]), ['flat'], 'flat', { x: 0, y: 0 })!;
    expect(moveTo(flat, { x: -10, y: 0 }, 1)).toEqual({ delta: { x: -10, y: 0 }, held: null });
  });
});

describe('the keys in 3D', () => {
  it('edit the picked parts as in 2D', () => {
    expect(editKey('r', false, true)).toEqual({ kind: 'rotate', degrees: 90 });
    expect(editKey('R', true, true)).toEqual({ kind: 'rotate', degrees: -90 });
    expect(editKey('f', false, true)).toEqual({ kind: 'flip' });
    expect(editKey('l', false, true)).toEqual({ kind: 'lock' });
    expect(editKey('ArrowUp', false, true)).toEqual({ kind: 'nudge', dx: 0, dy: -1, big: false });
    expect(editKey('ArrowRight', true, true)).toEqual({ kind: 'nudge', dx: 1, dy: 0, big: true });
  });

  it('are the view own keys with nothing picked, and Shift+F still turns the board over', () => {
    expect(editKey('f', false, false)).toBeNull();
    expect(editKey('ArrowUp', false, false)).toBeNull();
    expect(editKey('F', true, true)).toBeNull();
    expect(editKey('m', false, true)).toBeNull();
  });
});
