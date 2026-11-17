// The walkaround of the track tool (issue 726): while a track is drawn, the server says how the next segment goes round the
// copper from the track's last corner to the pointer (netsmith_router::walk). What the page does here is ask once at a time,
// newest place first, and say what it was asked.

import type {
  PcbCrossed,
  PcbPoint,
  PcbRerouted,
  PcbRouteNet,
  PcbRun,
  PcbShove,
  PcbShoveAnswer,
  PcbShoved,
  PcbWalk,
  PcbWalked,
  RoutesChange,
} from '../api';
import { drawChanges } from './routes';

const pointKey = (q: PcbPoint | null) => (q ? `${q.x},${q.y}` : '-');

/** What a walk is for, as a string: two asks with the same key have the same answer. */
export function walkKey(w: PcbWalk): string {
  const p = pointKey;
  return [
    w.board,
    w.net,
    w.layer,
    w.shove ? 'shove' : 'walk',
    w.width ?? '-',
    p(w.from),
    p(w.to),
    p(w.heading),
    w.track.map((q) => p(q)).join(';'),
    w.earlier.map((r) => `${r.layer}:${r.points.map(p).join(';')}`).join('/'),
    w.vias.map(p).join(';'),
  ].join('|');
}

/** A track being drawn on several layers (issue 1009): the runs before its last layer change, each on its layer, the vias at the
 * changes, and the run being drawn now. */
export interface Layered {
  net: string;
  layer: string;
  points: PcbPoint[];
  earlier: PcbRun[];
  vias: PcbPoint[];
}

/** The changes that draw a track of several layers: each run's segments on its layer and a via at each change. A via after the last
 * run's only corner (a layer changed and nothing drawn after) is left out. */
export function drawLayered(d: Layered, between?: (a: string, b: string) => string[] | null): RoutesChange[] {
  const runs = [...d.earlier, { layer: d.layer, points: d.points }];
  const vias = d.points.length < 2 && d.earlier.length > 0 ? d.vias.slice(0, -1) : d.vias;
  return [
    ...runs.flatMap((r) => drawChanges(d.net, r.layer, r.points)),
    // On a board with blind and buried vias, the via between the two runs' layers (issue 1071).
    ...vias.map((at, i): RoutesChange => {
      const layers = between?.(runs[i].layer, runs[i + 1].layer) ?? null;
      return { what: 'add_via', net: d.net, at, ...(layers ? { layers } : {}) };
    }),
  ];
}

/** A track changes layer at its last corner: a via goes there, and the run being drawn starts again from it on `layer`. Changed
 * back before a corner is drawn on the new layer, the via goes and the run before it is the one being drawn again. */
export function changeLayer<D extends Layered>(d: D, layer: string): D {
  if (d.layer === layer) return d;
  const before = d.earlier[d.earlier.length - 1];
  if (d.points.length < 2 && before && before.layer === layer) {
    return { ...d, layer, points: before.points, earlier: d.earlier.slice(0, -1), vias: d.vias.slice(0, -1) };
  }
  if (d.points.length < 2) return { ...d, layer };
  const at = d.points[d.points.length - 1];
  return { ...d, layer, points: [at], earlier: [...d.earlier, { layer: d.layer, points: d.points }], vias: [...d.vias, at] };
}

/** What a shove of a via or a drag is for, as a string. */
export function shoveKey(s: PcbShove): string {
  const p = pointKey;
  return [
    s.board,
    s.net,
    s.shove ? 'shove' : 'stay',
    s.tracks.map((t) => `${t.layer}:${t.width ?? '-'}:${p(t.from)}:${p(t.to)}`).join(';'),
    s.vias.map((v) => `${p(v.at)}:${v.diameter ?? '-'}`).join(';'),
    s.moving.map((m) => `${m.net}:${m.track ?? '-'}:${m.via ?? '-'}`).join(';'),
  ].join('|');
}

/** The answer to a shove of a via or a drag: why it may not go there (null: it may), what it moves aside, or why it could not. */
export interface ShoveAnswer {
  key: string;
  reason: string | null;
  shoved: PcbShoved;
  stuck: string | null;
}

/** What the server's answer to a shove, or its failure, says. */
export function shoveAnswerTo(s: PcbShove, got: PcbShoveAnswer | Error): ShoveAnswer {
  const key = shoveKey(s);
  if (got instanceof Error) return { key, reason: got.message, shoved: NOTHING_SHOVED, stuck: null };
  return { key, reason: got.reason ?? null, shoved: got.shoved ?? NOTHING_SHOVED, stuck: got.stuck ?? null };
}

/** The ask for a via of the process on `net` at `at`. */
export function viaShove(board: string, net: string, at: PcbPoint, shove: boolean, layers?: string[] | null): PcbShove {
  return { board, net, tracks: [], vias: [{ at, diameter: null, ...(layers ? { layers } : {}) }], moving: [], shove };
}

/** The ask for a drag: the shapes of `record` the changes of the drag (`dragChanges`) move, where they go, and which they are. */
export function dragShove(board: string, record: PcbRouteNet, changes: RoutesChange[], shove: boolean): PcbShove {
  const ask: PcbShove = { board, net: record.net, tracks: [], vias: [], moving: [], shove };
  for (const c of changes) {
    if (c.what === 'move_track') {
      const t = record.tracks[c.track - 1];
      if (!t) continue;
      ask.tracks.push({ layer: t.layer, width: t.width, from: c.from, to: c.to });
      ask.moving.push({ net: c.net, track: c.track, via: null });
    } else if (c.what === 'move_via') {
      const v = record.vias[c.via - 1];
      if (!v) continue;
      ask.vias.push({ at: c.at, diameter: v.diameter, ...(v.layers?.length ? { layers: v.layers } : {}) });
      ask.moving.push({ net: c.net, track: null, via: c.via });
    }
  }
  return ask;
}

/** The re-route asked for a walk answered (issue 1009): the run on the walk's layer with the way found, and the rest of the track. */
export function rerouteAsk(w: PcbWalk, points: PcbPoint[]): PcbWalk {
  return { ...w, track: [...w.track, ...points.slice(1)], to: w.from, heading: null, shove: false };
}

/** The router's crossed nets routed again round the track, as the page keeps it, with the ask it answers. */
export interface Rerouted {
  key: string;
  answer: PcbRerouted | null;
}

/** What a re-route says, as words: "routed again round it", or how many connections it could not make. */
export function reroutedText(r: PcbRerouted): string {
  if (r.left === 0) return `routed again round it (${r.nets.join(', ')})`;
  return `routed again round it, ${r.left} connection${r.left === 1 ? '' : 's'} left`;
}

/** The direction of the last segment of a track drawn through `points` (it arrives at the last point), or null while there is none
 * (one point, or the last points are the same place). */
export function headingOf(points: PcbPoint[]): PcbPoint | null {
  for (let i = points.length - 1; i > 0; i--) {
    const [a, b] = [points[i - 1], points[i]];
    if (Math.hypot(b.x - a.x, b.y - a.y) > 5e-4) return { x: b.x - a.x, y: b.y - a.y };
  }
  return null;
}

/** The answer to a walk: the corners from the last corner to the end, or why there are none. */
export interface Answer {
  key: string;
  points: PcbPoint[] | null;
  reason: string | null;
  /** The router's copper the track (the draft and this segment) crosses: copper the edit discards, not an obstacle. */
  crossed: PcbCrossed;
  /** The unlocked shapes of the routes file the track shoves out of its way, in their new places (none when the walk went round). */
  shoved: PcbShoved;
  /** Why nothing was shoved though something was in the way (the shape that could not move): the walk went round instead. */
  stuck: string | null;
}

export const NOTHING_CROSSED: PcbCrossed = { tracks: [], vias: [] };
export const NOTHING_SHOVED: PcbShoved = { tracks: [], vias: [] };

/** The changes that move the shapes a shove moved, to go in the same edit as the track drawn (the server makes them all or none,
 * with one check of the design rules, and one step of undo). */
export function shoveChanges(s: PcbShoved): RoutesChange[] {
  return [
    ...s.tracks.map((t): RoutesChange => ({ what: 'move_track', net: t.net, track: t.track, from: t.from, to: t.to, ...(t.mid ? { mid: t.mid } : {}) })),
    ...s.vias.map((v): RoutesChange => ({ what: 'move_via', net: v.net, via: v.via, at: v.at })),
  ];
}

/** How many shapes are shoved, as words ("2 tracks and 1 via"), or null when none is. */
export function shovedText(s: PcbShoved): string | null {
  const [t, v] = [s.tracks.length, s.vias.length];
  const part = (n: number, one: string) => (n === 0 ? null : `${n} ${one}${n === 1 ? '' : 's'}`);
  const parts = [part(t, 'track'), part(v, 'via')].filter(Boolean);
  return parts.length ? parts.join(' and ') : null;
}

/** What an edit of the routes file says when the board was routed: the router's copper is thrown away with it (the board is made
 * again from the file, unrouted), so the person is told, and that routing again lays it round the new copper. */
export function saidOf(said: string, wasRouted: boolean): string {
  return wasRouted ? `${said}; the router's copper was discarded: route again to lay it round the new copper` : said;
}

/** How many pieces of the router's copper are crossed, as words ("2 router tracks and 1 via"), or null when none is. */
export function crossedText(c: PcbCrossed): string | null {
  const [t, v] = [c.tracks.length, c.vias.length];
  const part = (n: number, one: string) => (n === 0 ? null : `${n} ${one}${n === 1 ? '' : 's'}`);
  const parts = [part(t, 'router track'), part(v, 'via')].filter(Boolean);
  return parts.length ? parts.join(' and ') : null;
}

/** What the server's answer, or its failure, says of the walk asked. */
export function answerTo(w: PcbWalk, got: PcbWalked | Error): Answer {
  const key = walkKey(w);
  if (got instanceof Error) return { key, points: null, reason: got.message, crossed: NOTHING_CROSSED, shoved: NOTHING_SHOVED, stuck: null };
  return {
    key,
    points: got.points && got.points.length >= 2 ? got.points : null,
    reason: got.reason ?? (got.points ? null : 'no way found'),
    crossed: got.crossed ?? NOTHING_CROSSED,
    shoved: got.shoved ?? NOTHING_SHOVED,
    stuck: got.stuck ?? null,
  };
}

/** `run` for the latest ask only: at most one is running, and an ask that comes while it runs replaces any that waits (a
 * pointer that moves faster than the server answers asks for where it is, not for where it has been). `done` gets each
 * answer that was run, with its ask. Returns the function to ask with. */
export function latestOnly<A, R>(run: (a: A) => Promise<R>, done: (a: A, r: R | Error) => void): (a: A) => void {
  let running = false;
  let next: { a: A } | null = null;
  const pump = async () => {
    running = true;
    try {
      while (next) {
        const { a } = next;
        next = null;
        let r: R | Error;
        try {
          r = await run(a);
        } catch (e) {
          r = e instanceof Error ? e : new Error(String(e));
        }
        done(a, r);
      }
    } finally {
      running = false;
    }
  };
  return (a) => {
    next = { a };
    if (!running) void pump();
  };
}
