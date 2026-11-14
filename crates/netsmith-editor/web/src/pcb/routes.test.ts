import { describe, expect, it } from 'vitest';
import type { PcbRouteNet } from '../api';
import { boxOf, deleteChange, lockChange, lockedOf, nameOf, trackPath } from './routes';

const net: PcbRouteNet = {
  net: 'VCC',
  lock: true,
  current: true,
  tracks: [
    { layer: 'top', width: 0.3, from: { x: 0, y: 0 }, mid: null, to: { x: 2, y: 0 }, lock: false, drawn: true },
    { layer: 'top', width: 0.3, from: { x: 0, y: 0 }, mid: { x: 1, y: -1 }, to: { x: 2, y: 0 }, lock: true, drawn: true },
  ],
  vias: [{ at: { x: 5, y: 5 }, diameter: 0.6, drill: 0.3, lock: false, drawn: true }],
};

describe('the routes file in the view', () => {
  it('draws a line, and an arc through its middle', () => {
    expect(trackPath(net.tracks[0])).toBe('M 0 0 L 2 0');
    // A half circle of radius 1 over the top (y up on the screen is negative): clockwise on the screen, not the long way.
    expect(trackPath(net.tracks[1])).toBe('M 0 0 A 1 1 0 0 1 2 0');
    // The same ends through a point below: the other way round.
    expect(trackPath({ ...net.tracks[1], mid: { x: 1, y: 1 } })).toBe('M 0 0 A 1 1 0 0 0 2 0');
    // Three quarters of a circle: the long way.
    const r = Math.SQRT1_2;
    expect(trackPath({ from: { x: 0, y: 0 }, mid: { x: 0.5, y: -0.5 - r }, to: { x: 1, y: 0 } })).toMatch(/ A [\d.]+ [\d.]+ 0 1 1 1 0$/);
  });

  it('names a shape and tells whose lock holds', () => {
    expect(nameOf({ net: 'VCC', track: 2 })).toBe('track 2 of net VCC');
    expect(nameOf({ net: 'VCC', via: 1 })).toBe('via 1 of net VCC');
    expect(nameOf({ net: 'VCC' })).toBe('net VCC');
    expect(lockedOf(net, { net: 'VCC', track: 1 })).toBe(false);
    expect(lockedOf(net, { net: 'VCC', track: 2 })).toBe(true);
    expect(lockedOf(net, { net: 'VCC' })).toBe(true);
  });

  it('makes the change the routes commands make', () => {
    expect(lockChange({ net: 'VCC', via: 1 }, true)).toEqual({ what: 'lock', net: 'VCC', track: null, via: 1, lock: true });
    expect(deleteChange({ net: 'VCC' })).toEqual({ what: 'delete', net: 'VCC', track: null, via: null });
  });

  it('zooms to a shape, or to the whole record', () => {
    const via = boxOf(net, { net: 'VCC', via: 1 })!;
    expect(via.x).toBeCloseTo(4.4);
    expect(via.width).toBeCloseTo(1.2);
    const all = boxOf(net, { net: 'VCC' })!;
    expect(all.x).toBe(-0.5);
    expect(all.x + all.width).toBe(5.5);
  });
});
