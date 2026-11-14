// The walkaround of the track tool (issue 726): while a track is drawn, the server says how the next segment goes round the
// copper from the track's last corner to the pointer (netsmith_router::walk). What the page does here is ask once at a time,
// newest place first, and say what it was asked.

import type { PcbPoint, PcbWalk, PcbWalked } from '../api';

/** What a walk is for, as a string: two asks with the same key have the same answer. */
export function walkKey(w: PcbWalk): string {
  const p = (q: PcbPoint | null) => (q ? `${q.x},${q.y}` : '-');
  return [w.board, w.net, w.layer, w.width ?? '-', p(w.from), p(w.to), p(w.heading)].join('|');
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
}

/** What the server's answer, or its failure, says of the walk asked. */
export function answerTo(w: PcbWalk, got: PcbWalked | Error): Answer {
  const key = walkKey(w);
  if (got instanceof Error) return { key, points: null, reason: got.message };
  return { key, points: got.points && got.points.length >= 2 ? got.points : null, reason: got.reason ?? (got.points ? null : 'no way found') };
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
