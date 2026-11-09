// Height limits on the PCB canvas (ADR 0045 section 11, PROVISIONAL): which limit a part stands
// under, what the part's hover says about it, and how far a drag may go before a part that is too
// tall would stand under a limit it is over. Plain numbers in millimetres, no DOM, so it is
// tested on its own. The rules are the placement check's (`Board::check_placement`): the body is
// measured, a part with no stated height is not too tall (nobody knows), and a limit applies to
// the faces its layers name.

import type { PcbBox, PcbKeepout, PcbPart } from '../api';
import { type Pt, moved } from './geometry';

/** The check ignores a part over by half a micrometre or less. */
const TOLERANCE = 0.0005;

/** Whether two boxes share area: boxes that only touch do not, as in the placement check. */
function overlap(a: PcbBox, b: PcbBox): boolean {
  const e = 1e-6;
  return a.x < b.x + b.width - e && b.x < a.x + a.width - e && a.y < b.y + b.height - e && b.y < a.y + a.height - e;
}

/** A keepout that limits height, with the box it limits. */
export interface Zone {
  name: string;
  box: PcbBox;
  limit: number;
  layers: string[];
}

/** The zones of a scene: its keepouts that state a limit and have a place. */
export function zonesOf(keepouts: PcbKeepout[]): Zone[] {
  return keepouts.flatMap((k) => (k.maxHeight != null && k.area ? [{ name: k.name, box: k.area, limit: k.maxHeight, layers: k.layers }] : []));
}

/** Whether the zone limits parts on the part's face. */
export function limitsFace(zone: Zone, part: Pick<PcbPart, 'side'>): boolean {
  return zone.layers.includes(part.side);
}

/** The zones a part's body overlaps (shifted by `by`) and that limit its face. */
export function under(part: PcbPart, zones: Zone[], by: Pt = { x: 0, y: 0 }): Zone[] {
  const body = moved(part.body, by);
  return zones.filter((z) => limitsFace(z, part) && overlap(body, z.box));
}

/** The zones a part is too tall for where it is (shifted by `by`): its height is stated and over
 * the limit. */
export function tooTallFor(part: PcbPart, zones: Zone[], by: Pt = { x: 0, y: 0 }): Zone[] {
  const h = part.height;
  return h == null ? [] : under(part, zones, by).filter((z) => h > z.limit + TOLERANCE);
}

function mm(v: number): string {
  return `${Math.round(v * 100) / 100} mm`;
}

/** The lines of a part's hover about its height: how tall it stands, and for each limit it
 * stands under what the limit allows and whether it is over. */
export function heightLines(part: PcbPart, zones: Zone[]): string[] {
  const lines: string[] = [];
  const h = part.height;
  lines.push(h == null ? 'height not stated' : `stands ${mm(h)} tall`);
  for (const z of under(part, zones)) {
    if (h == null) lines.push(`${z.name} allows ${mm(z.limit)} here: unchecked`);
    else if (h > z.limit + TOLERANCE) lines.push(`${z.name} allows ${mm(z.limit)} here: ${mm(h - z.limit)} too tall`);
    else lines.push(`${z.name} allows ${mm(z.limit)} here: fits`);
  }
  return lines;
}

/** A drag held by a limit: the zone, the part that would be too tall and its height. */
export interface Held {
  zone: string;
  limit: number;
  refdes: string;
  height: number;
}

/** The pairs `part|zone` a set of parts is already too tall for at `by`: a part that starts under a
 * limit it is over may stay under it (and move out), but may not go under another. */
function pairs(parts: PcbPart[], zones: Zone[], by: Pt): Set<string> {
  const found = new Set<string>();
  for (const p of parts) for (const z of tooTallFor(p, zones, by)) found.add(`${p.path}|${z.name}`);
  return found;
}

function newOnes(parts: PcbPart[], zones: Zone[], by: Pt, allowed: Set<string>): string | null {
  for (const key of pairs(parts, zones, by)) if (!allowed.has(key)) return key;
  return null;
}

/** How far the parts may move toward `want` from `last` (the last move that was allowed; both are
 * on the grid): they go step by step, `step` mm at a time, along x and then y or along y and then
 * x, whichever ends nearer to `want`, and stop at the first step that would put a part under a
 * limit it is too tall for. So a part held by a limit slides along its edge instead of sticking
 * to it, and a quick drag across a thin zone does not jump over it. Returns the move and, if it
 * is not `want`, what held it. */
export function limitMove(parts: PcbPart[], zones: Zone[], want: Pt, last: Pt, step: number): { delta: Pt; held: Held | null } {
  const tall = parts.filter((p) => p.height != null);
  if (tall.length === 0 || zones.length === 0) return { delta: want, held: null };
  const start = pairs(tall, zones, { x: 0, y: 0 });
  const ok = (by: Pt) => newOnes(tall, zones, by, start) === null;
  const grain = step > 0 ? step : 0.05;
  const walk = (from: Pt, axis: 'x' | 'y', to: number): Pt => {
    let at = from;
    const dist = to - from[axis];
    const n = Math.ceil(Math.abs(dist) / grain - 1e-9);
    for (let i = 1; i <= n; i++) {
      const v = i === n ? to : from[axis] + (Math.sign(dist) * i * grain);
      const next = { ...at, [axis]: Math.round(v * 1e4) / 1e4 } as Pt;
      if (!ok(next)) return at;
      at = next;
    }
    return at;
  };
  const route = (first: 'x' | 'y'): Pt => {
    const second = first === 'x' ? 'y' : 'x';
    return walk(walk(last, first, want[first]), second, want[second]);
  };
  const off = (p: Pt) => Math.hypot(p.x - want.x, p.y - want.y);
  const a = route('x');
  const b = route('y');
  const delta = off(b) < off(a) ? b : a;
  if (delta.x === want.x && delta.y === want.y) return { delta, held: null };
  // What stopped it: the first limit it would be over one step further toward `want`.
  const toward = { x: delta.x + Math.sign(want.x - delta.x) * grain, y: delta.y + Math.sign(want.y - delta.y) * grain };
  const key = newOnes(tall, zones, toward, start) ?? newOnes(tall, zones, { x: toward.x, y: delta.y }, start) ?? newOnes(tall, zones, { x: delta.x, y: toward.y }, start);
  const [path, zone] = (key ?? '|').split('|');
  const part = tall.find((p) => p.path === path);
  const z = zones.find((q) => q.name === zone);
  return { delta, held: part && z ? { zone: z.name, limit: z.limit, refdes: part.refdes, height: part.height ?? 0 } : null };
}

/** What the tip says while a drag is held. */
export function heldTip(h: Held): { title: string; lines: string[] } {
  return { title: `limited by ${h.zone}`, lines: [`${h.refdes} stands ${mm(h.height)} tall`, `${h.zone} allows ${mm(h.limit)}`] };
}
