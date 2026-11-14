import { describe, expect, it } from 'vitest';
import type { PcbRouteNet } from '../api';
import {
  anchorsOf,
  boxOf,
  deleteChange,
  dragChanges,
  drawChanges,
  endNear,
  labelOf,
  lockChange,
  lockedOf,
  nameOf,
  nearestAnchor,
  trackPath,
} from './routes';

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

describe('drawing and moving copper', () => {
  const corner: PcbRouteNet = {
    net: 'GND',
    lock: false,
    current: true,
    tracks: [
      { layer: 'top', width: 0.2, from: { x: 0, y: 0 }, mid: null, to: { x: 1, y: 1 }, lock: false, drawn: true },
      { layer: 'top', width: 0.2, from: { x: 1, y: 1 }, mid: null, to: { x: 3, y: 1 }, lock: false, drawn: true },
      { layer: 'top', width: 0.2, from: { x: 3, y: 1 }, mid: null, to: { x: 4, y: 1 }, lock: false, drawn: false },
    ],
    vias: [{ at: { x: 1, y: 1 }, diameter: 0.6, drill: 0.3, lock: false, drawn: true }],
  };

  it('draws one straight track for each segment, at the net width of the rules, and none of no length', () => {
    const pts = [{ x: 0, y: 0 }, { x: 1, y: 1 }, { x: 1, y: 1 }, { x: 3, y: 1 }];
    expect(drawChanges('VCC', 'bottom', pts)).toEqual([
      { what: 'add_track', net: 'VCC', layer: 'bottom', width: null, from: pts[0], to: pts[1] },
      { what: 'add_track', net: 'VCC', layer: 'bottom', width: null, from: pts[2], to: pts[3] },
    ]);
    expect(drawChanges('VCC', 'top', [pts[0]])).toEqual([]);
  });

  it('moves a corner with every track and via that meets there', () => {
    const to = { x: 1, y: 2 };
    expect(dragChanges(corner, { x: 1, y: 1 }, to)).toEqual([
      { what: 'move_track', net: 'GND', track: 1, from: { x: 0, y: 0 }, to },
      { what: 'move_track', net: 'GND', track: 2, from: to, to: { x: 3, y: 1 } },
      { what: 'move_via', net: 'GND', via: 1, at: to },
    ]);
    // A free end moves one track; a drag back to where it was is no change.
    expect(dragChanges(corner, { x: 0, y: 0 }, to)).toHaveLength(1);
    expect(dragChanges(corner, { x: 1, y: 1 }, { x: 1.0001, y: 1 })).toEqual([]);
  });

  it('finds the end of a track and the anchor nearest a point, of a net when named', () => {
    expect(endNear(corner.tracks[1], { x: 2.9, y: 1.05 }, 0.2)).toEqual({ x: 3, y: 1 });
    expect(endNear(corner.tracks[1], { x: 2, y: 1 }, 0.2)).toBeNull();
    const anchors = anchorsOf({
      parts: [{ pads: [{ number: '1', x: 1.1, y: 1, net: 'VCC' }, { number: '2', x: 5, y: 5, net: null }] }] as never,
      routes: [corner],
    });
    // The pad with no net is no anchor, nor the end of a track not drawn.
    expect(anchors.filter((a) => a.what === 'pad')).toHaveLength(1);
    expect(anchors.some((a) => a.at.x === 4)).toBe(false);
    expect(nearestAnchor(anchors, { x: 1.08, y: 1 }, 0.5)?.net).toBe('VCC');
    expect(nearestAnchor(anchors, { x: 1.08, y: 1 }, 0.5, 'GND')?.what).not.toBe('pad');
    expect(nearestAnchor(anchors, { x: 9, y: 9 }, 0.5)).toBeNull();
  });

  it('names an edit for the undo history', () => {
    expect(labelOf([lockChange({ net: 'VCC', track: 2 }, false)])).toBe('unlock track 2 of net VCC');
    expect(labelOf(drawChanges('VCC', 'top', [{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 1, y: 2 }]))).toBe('draw a track of 2 segments on net VCC');
    expect(labelOf([{ what: 'move_via', net: 'GND', via: 1, at: { x: 0, y: 0 } }])).toBe('move via 1 of net GND');
    expect(labelOf(dragChanges(corner, { x: 1, y: 1 }, { x: 2, y: 2 }))).toBe('move 3 shapes of net GND');
  });
});
