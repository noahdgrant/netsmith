// The routes file's copper as the PCB view draws and edits it (issue 599): where a shape is, the
// path of an arc, what a shape is called, and the changes (`RoutesChange`) that a lock, a delete,
// a track drawn, a via placed and a drag are. The server makes them as `netsmith routes` makes
// them and refuses what that refuses.

import type { PcbBox, PcbPoint, PcbRouteNet, PcbRouteTrack, PcbRouteVia, PcbScene, RoutesChange } from '../api';
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

/** What an edit of the routes file is called in the undo history and its toast. */
export function labelOf(changes: RoutesChange[]): string {
  const c = changes[0];
  if (!c) return 'nothing';
  const pick = (x: { net: string; track?: number | null; via?: number | null }): RoutePick => ({
    net: x.net,
    ...(x.track != null ? { track: x.track } : {}),
    ...(x.via != null ? { via: x.via } : {}),
  });
  switch (c.what) {
    case 'delete':
      return `delete ${nameOf(pick(c))}`;
    case 'lock':
      return `${c.lock ? 'lock' : 'unlock'} ${nameOf(pick(c))}`;
    case 'add_track':
      return changes.length > 1 ? `draw a track of ${changes.length} segments on net ${c.net}` : `draw a track on net ${c.net}`;
    case 'add_via':
      return `place a via on net ${c.net}`;
    case 'move_track':
    case 'move_via':
      return changes.length > 1 ? `move ${changes.length} shapes of net ${c.net}` : `move ${nameOf(pick(c))}`;
    case 'restore':
      return `put back net ${c.net}'s record`;
  }
}

/** The same place, to the micrometre: where two shapes of a record meet. */
export function samePoint(a: PcbPoint, b: PcbPoint): boolean {
  return Math.abs(a.x - b.x) < 5e-4 && Math.abs(a.y - b.y) < 5e-4;
}

/** A place a track starts or ends at, or a via goes on: a pad of a net, an end of a track or a via of the routes file. */
export interface Anchor {
  at: PcbPoint;
  net: string;
  what: 'pad' | 'track' | 'via';
}

/** The anchors of the board: every pad with a net, and the ends of the drawn tracks and the drawn vias of the routes file. */
export function anchorsOf(sc: Pick<PcbScene, 'parts' | 'routes'>): Anchor[] {
  const out: Anchor[] = [];
  for (const p of sc.parts) for (const pad of p.pads) if (pad.net) out.push({ at: { x: pad.x, y: pad.y }, net: pad.net, what: 'pad' });
  for (const n of sc.routes) {
    for (const t of n.tracks) if (t.drawn) out.push({ at: t.from, net: n.net, what: 'track' }, { at: t.to, net: n.net, what: 'track' });
    for (const v of n.vias) if (v.drawn) out.push({ at: v.at, net: n.net, what: 'via' });
  }
  return out;
}

/** The anchor nearest `p` within `reach` mm, of `net` when one is named. */
export function nearestAnchor(anchors: Anchor[], p: PcbPoint, reach: number, net?: string): Anchor | null {
  let best: Anchor | null = null;
  let d = reach;
  for (const a of anchors) {
    if (net != null && a.net !== net) continue;
    const e = Math.hypot(a.at.x - p.x, a.at.y - p.y);
    if (e <= d) {
      best = a;
      d = e;
    }
  }
  return best;
}

/** The changes that draw a track of `net` on `layer` through `points`: one straight track for each segment, a segment of no
 * length left out. The width is the net's by the board's rules (the server's). */
export function drawChanges(net: string, layer: string, points: PcbPoint[]): RoutesChange[] {
  const out: RoutesChange[] = [];
  for (let i = 1; i < points.length; i++) {
    const [from, to] = [points[i - 1], points[i]];
    if (!samePoint(from, to)) out.push({ what: 'add_track', net, layer, width: null, from, to });
  }
  return out;
}

/** The end of a straight track within `reach` of `p`, the nearer of the two. */
export function endNear(t: PcbRouteTrack, p: PcbPoint, reach: number): PcbPoint | null {
  const df = Math.hypot(t.from.x - p.x, t.from.y - p.y);
  const dt = Math.hypot(t.to.x - p.x, t.to.y - p.y);
  if (Math.min(df, dt) > reach) return null;
  return df <= dt ? t.from : t.to;
}

/** The changes that move the place `from` of a net's record to `to`: every track of the record with an end there and every via
 * there move with it, so a corner stays joined. The server refuses a locked shape or an arc among them, with the reason. */
export function dragChanges(record: PcbRouteNet, from: PcbPoint, to: PcbPoint): RoutesChange[] {
  if (samePoint(from, to)) return [];
  const out: RoutesChange[] = [];
  record.tracks.forEach((t, i) => {
    const a = samePoint(t.from, from);
    const b = samePoint(t.to, from);
    if (a || b) out.push({ what: 'move_track', net: record.net, track: i + 1, from: a ? to : t.from, to: b ? to : t.to });
  });
  record.vias.forEach((v, i) => {
    if (samePoint(v.at, from)) out.push({ what: 'move_via', net: record.net, via: i + 1, at: to });
  });
  return out;
}

/** A place on the sheet moved from `from` to `to` if it is at `from`: a shape's point as a drag shows it. */
export function dragged(p: PcbPoint, drag: { from: PcbPoint; to: PcbPoint } | null): PcbPoint {
  return drag && samePoint(p, drag.from) ? drag.to : p;
}
