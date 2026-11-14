// The routes file's copper as the PCB view draws and edits it (issue 599): where a shape is, the
// path of an arc, what a shape is called, and the change (`RoutesChange`) a lock or a delete is.
// The server makes the change as `netsmith routes` makes it and refuses what that refuses.

import type { PcbBox, PcbPoint, PcbRouteNet, PcbRouteTrack, PcbRouteVia, RoutesChange } from '../api';
import type { RoutePick } from '../pcbstore';

/** The SVG path of a track: a line, or the arc through `mid` (the circle through the three points). */
export function trackPath(t: Pick<PcbRouteTrack, 'from' | 'mid' | 'to'>): string {
  const { from: a, to: c } = t;
  const b = t.mid;
  if (!b) return `M ${a.x} ${a.y} L ${c.x} ${c.y}`;
  const d = 2 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
  if (Math.abs(d) < 1e-12) return `M ${a.x} ${a.y} L ${c.x} ${c.y}`;
  const sq = (p: PcbPoint) => p.x * p.x + p.y * p.y;
  const ux = (sq(a) * (b.y - c.y) + sq(b) * (c.y - a.y) + sq(c) * (a.y - b.y)) / d;
  const uy = (sq(a) * (c.x - b.x) + sq(b) * (a.x - c.x) + sq(c) * (b.x - a.x)) / d;
  const r = Math.hypot(a.x - ux, a.y - uy);
  // SVG's sweep 1 is clockwise on the screen (y down), as a to b to c turns when their cross product is positive.
  const cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
  const sweep = cross > 0 ? 1 : 0;
  // The arc spans more than half the circle when the centre is on mid's side of the chord.
  const large = sideOf(a, c, b) === sideOf(a, c, { x: ux, y: uy }) ? 1 : 0;
  return `M ${a.x} ${a.y} A ${r} ${r} 0 ${large} ${sweep} ${c.x} ${c.y}`;
}

function sideOf(a: PcbPoint, c: PcbPoint, p: PcbPoint): number {
  return Math.sign((c.x - a.x) * (p.y - a.y) - (c.y - a.y) * (p.x - a.x));
}

/** Where a track's middle is: the arc's own middle, or the line's. */
export function middleOf(t: PcbRouteTrack): PcbPoint {
  return t.mid ?? { x: (t.from.x + t.to.x) / 2, y: (t.from.y + t.to.y) / 2 };
}

function boxAround(points: PcbPoint[], pad: number): PcbBox | null {
  if (points.length === 0) return null;
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  const [x0, y0] = [Math.min(...xs) - pad, Math.min(...ys) - pad];
  return { x: x0, y: y0, width: Math.max(...xs) + pad - x0, height: Math.max(...ys) + pad - y0 };
}

/** The box round a picked record or shape, to zoom to. */
export function boxOf(net: PcbRouteNet, pick: RoutePick): PcbBox | null {
  const track = pick.track != null ? net.tracks[pick.track - 1] : undefined;
  const via = pick.via != null ? net.vias[pick.via - 1] : undefined;
  if (track) return boxAround([track.from, track.to, ...(track.mid ? [track.mid] : [])], track.width);
  if (via) return boxAround([via.at], via.diameter);
  return boxAround(
    [...net.tracks.flatMap((t) => [t.from, t.to]), ...net.vias.map((v) => v.at)],
    0.5,
  );
}

/** The shape a pick names, if it names one. */
export function shapeOf(net: PcbRouteNet, pick: RoutePick): PcbRouteTrack | PcbRouteVia | null {
  if (pick.track != null) return net.tracks[pick.track - 1] ?? null;
  if (pick.via != null) return net.vias[pick.via - 1] ?? null;
  return null;
}

/** What a pick is called in a message: `track 2 of net VCC`, `net VCC`. */
export function nameOf(pick: RoutePick): string {
  if (pick.track != null) return `track ${pick.track} of net ${pick.net}`;
  if (pick.via != null) return `via ${pick.via} of net ${pick.net}`;
  return `net ${pick.net}`;
}

/** Whether what a pick names is locked: the shape's lock, or the net's. */
export function lockedOf(net: PcbRouteNet, pick: RoutePick): boolean {
  const shape = shapeOf(net, pick);
  return shape ? shape.lock : net.lock;
}

/** The change that locks (or unlocks) what a pick names. */
export function lockChange(pick: RoutePick, lock: boolean): RoutesChange {
  return { what: 'lock', net: pick.net, track: pick.track ?? null, via: pick.via ?? null, lock };
}

/** The change that deletes what a pick names: one shape, or the net's whole record. */
export function deleteChange(pick: RoutePick): RoutesChange {
  return { what: 'delete', net: pick.net, track: pick.track ?? null, via: pick.via ?? null };
}

/** The pick a drawn shape of the overlay names (`data-route-net` with `data-route-track` or
 * `data-route-via`), if the element is in one. */
export function pickAt(target: Element | null): RoutePick | null {
  const el = target?.closest('[data-route-net]');
  if (!el) return null;
  const net = el.getAttribute('data-route-net')!;
  const track = el.getAttribute('data-route-track');
  const via = el.getAttribute('data-route-via');
  return { net, ...(track ? { track: Number(track) } : {}), ...(via ? { via: Number(via) } : {}) };
}

export function samePick(a: RoutePick | null, b: RoutePick | null): boolean {
  return !!a && !!b && a.net === b.net && a.track === b.track && a.via === b.via;
}
