// The walkaround of the track tool (issue 726): while a track is drawn, the server says how the next segment goes round the
// copper from the track's last corner to the pointer (netsmith_router::walk). What the page does here is ask once at a time,
// newest place first, and say what it was asked.

import type { PcbCrossed, PcbPoint, PcbShoved, PcbWalk, PcbWalked, RoutesChange } from '../api';

/** What a walk is for, as a string: two asks with the same key have the same answer. */
export function walkKey(w: PcbWalk): string {
  const p = (q: PcbPoint | null) => (q ? `${q.x},${q.y}` : '-');
  return [w.board, w.net, w.layer, w.shove ? 'shove' : 'walk', w.width ?? '-', p(w.from), p(w.to), p(w.heading), w.track.map((q) => p(q)).join(';')].join('|');
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
    ...s.tracks.map((t): RoutesChange => ({ what: 'move_track', net: t.net, track: t.track, from: t.from, to: t.to })),
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
