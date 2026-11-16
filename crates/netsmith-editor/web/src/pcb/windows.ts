// Placement requirements on the PCB canvas (ADR 0045 C4, issue 815): the window each part's
// requirements leave its body, the ghost of the body at the nearest place they allow, how far a
// drag may go before a part leaves its window, and what "Require here" writes. Plain numbers in
// millimetres, no DOM, so it is tested on its own. The rules are the placer's and the check's
// (`netsmith_pcb::constraints`): `x` and `y` are the centre of the body, `left`, `right`, `top`
// and `bottom` its edges, all from the outline's top left corner, and the body is the box the
// check measures, as it is turned now.

import type { PcbBox, PcbPart, PcbRequirement, RequiredPlace } from '../api';
import type { Pt } from './geometry';

/** Where the layout file's coordinates start on the sheet: the outline's top left corner. */
interface Frame {
  originX: number;
  originY: number;
}

/** The check lets a part be off by half a micrometre. */
const TOLERANCE = 0.0005;

/** The range of one edge of the body (sheet coordinates) and the requirements that bound each end. */
interface Span {
  lo: number;
  hi: number;
  loBy: PcbRequirement | null;
  hiBy: PcbRequirement | null;
}

/** The ranges the requirements leave the body's left edge (`x`) and top edge (`y`), on the sheet. */
export interface Window {
  x: Span;
  y: Span;
}

const ALONG_X = ['x', 'left', 'right'];
const ALONG_Y = ['y', 'top', 'bottom'];

/** How far the measured field is from the body's left (or top) edge, for a body `size` wide (or high). */
function offsetOf(field: string, size: number): number {
  if (field === 'x' || field === 'y') return size / 2;
  if (field === 'right' || field === 'bottom') return size;
  return 0;
}

function spanOf(reqs: PcbRequirement[], fields: string[], size: number, origin: number): Span | null {
  const mine = reqs.filter((r) => fields.includes(r.field));
  if (mine.length === 0) return null;
  const s: Span = { lo: -Infinity, hi: Infinity, loBy: null, hiBy: null };
  for (const r of mine) {
    const off = offsetOf(r.field, size);
    if (r.lo != null && r.lo - off + origin > s.lo) {
      s.lo = r.lo - off + origin;
      s.loBy = r;
    }
    if (r.hi != null && r.hi - off + origin < s.hi) {
      s.hi = r.hi - off + origin;
      s.hiBy = r;
    }
  }
  if (s.lo === -Infinity && s.hi === Infinity) return null;
  return s;
}

/** The window of a part's body, or `null` if its requirements say nothing of where it is (only its turn, or none). */
export function windowOf(part: PcbPart, f: Frame): Window | null {
  const b = part.body;
  const x = spanOf(part.requirements, ALONG_X, b.width, f.originX);
  const y = spanOf(part.requirements, ALONG_Y, b.height, f.originY);
  if (!x && !y) return null;
  const open: Span = { lo: -Infinity, hi: Infinity, loBy: null, hiBy: null };
  return { x: x ?? open, y: y ?? open };
}

/** The box the window lets the body stand in, on the sheet: an axis no requirement bounds spans the outline. */
export function windowBox(part: PcbPart, w: Window, outline: PcbBox): PcbBox {
  const b = part.body;
  const ends = (s: Span, size: number, from: number, length: number): [number, number] => {
    const a = Number.isFinite(s.lo) ? s.lo : Math.min(from, s.hi);
    const z = Number.isFinite(s.hi) ? s.hi + size : Math.max(from + length, s.lo + size);
    // Two requirements that leave no place: the box between them, as thin as it is.
    return a <= z ? [a, z] : [z, a];
  };
  const [x0, x1] = ends(w.x, b.width, outline.x, outline.width);
  const [y0, y1] = ends(w.y, b.height, outline.y, outline.height);
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

function clampTo(v: number, s: Span): number {
  if (s.lo > s.hi) return (s.lo + s.hi) / 2;
  return Math.min(Math.max(v, s.lo), s.hi);
}

/** The body at the nearest place its window allows, if it is not in its window now (by more than the check lets pass). */
export function ghostOf(part: PcbPart, w: Window): PcbBox | null {
  const b = part.body;
  const x = clampTo(b.x, w.x);
  const y = clampTo(b.y, w.y);
  if (Math.abs(x - b.x) <= TOLERANCE && Math.abs(y - b.y) <= TOLERANCE) return null;
  return { ...b, x, y };
}

/** A drag held by a requirement: the requirement, and the part it holds. */
export interface Held {
  requirement: PcbRequirement;
  refdes: string;
}

const up100 = (v: number) => Math.ceil(v * 100 - 1e-6) / 100;
const down100 = (v: number) => Math.floor(v * 100 + 1e-6) / 100;
const tidy = (v: number) => Math.round(v * 100) / 100;

/** The range of moves along one axis that keeps a part in its window, as the layout file keeps the
 * origin (to a hundredth of a millimetre, rounded inward so the saved place is still inside). A
 * part outside its window may move toward it and into it, never further out. */
function movesAlong(s: Span, origin: number, bodyEdge: number, frame: number): { lo: number; hi: number } {
  const at = tidy(origin - frame);
  const off = bodyEdge - origin;
  let lo = Number.isFinite(s.lo) ? up100(s.lo - off - frame) - at : -Infinity;
  let hi = Number.isFinite(s.hi) ? down100(s.hi - off - frame) - at : Infinity;
  if (lo > hi) {
    // Narrower than the file's hundredth: it stays, or goes to the middle if it is outside.
    const mid = (s.lo + s.hi) / 2 - off - frame - at;
    lo = Math.min(mid, 0);
    hi = Math.max(mid, 0);
    if (Math.abs(mid) <= 0.005) lo = hi = 0;
  }
  return { lo: Math.min(lo, 0), hi: Math.max(hi, 0) };
}

/** How far the parts may move toward `want` (a move, in mm) with every part kept in its window, axis by
 * axis, and, if the move is not `want`, the requirement that held it. */
export function windowMove(parts: PcbPart[], f: Frame, want: Pt): { delta: Pt; held: Held | null } {
  // The requirement that took the most off the move names it: the grid may nudge the other axis a little.
  let held: Held | null = null;
  let most = 0;
  const hold = (by: PcbRequirement | null, p: PcbPart, cut: number) => {
    if (by && cut > most) {
      most = cut;
      held = { requirement: by, refdes: p.refdes };
    }
  };
  const delta = { ...want };
  for (const axis of ['x', 'y'] as const) {
    for (const p of parts) {
      const w = windowOf(p, f);
      if (!w) continue;
      const s = w[axis];
      const m = movesAlong(s, p[axis], p.body[axis], axis === 'x' ? f.originX : f.originY);
      if (delta[axis] < m.lo - 1e-9) {
        hold(s.loBy, p, m.lo - delta[axis]);
        delta[axis] = m.lo;
      } else if (delta[axis] > m.hi + 1e-9) {
        hold(s.hiBy, p, delta[axis] - m.hi);
        delta[axis] = m.hi;
      }
    }
  }
  return { delta, held };
}

function num(v: number): string {
  return `${Math.round(v * 1e4) / 1e4}`;
}

/** A requirement's window as a person reads it: `1.4 to 1.6 mm`, `at least 2 mm`, `90 degrees`. */
export function rangeText(r: PcbRequirement): string {
  const unit = r.field === 'rotation' ? 'degrees' : 'mm';
  if (r.lo != null && r.hi != null) return Math.abs(r.hi - r.lo) < 1e-9 ? `${num(r.lo)} ${unit}` : `${num(r.lo)} to ${num(r.hi)} ${unit}`;
  if (r.lo != null) return `at least ${num(r.lo)} ${unit}`;
  if (r.hi != null) return `at most ${num(r.hi)} ${unit}`;
  return 'anything';
}

/** A requirement named with its line: `power.place.left (main.physical.nsm:16)`. */
export function nameOf(r: PcbRequirement): string {
  return r.source ? `${r.id} (${r.source})` : r.id;
}

/** The lines of a part's hover about its requirements: each with what it allows. */
export function requirementLines(part: PcbPart): string[] {
  return part.requirements.map((r) => `${r.id} allows ${rangeText(r)}${r.module ? ', for every part of the module' : ''}`);
}

/** What the tip says while a drag is held. */
export function heldTip(h: Held): { title: string; lines: string[] } {
  return { title: `limited by ${nameOf(h.requirement)}`, lines: [`${h.refdes}: ${h.requirement.id} allows ${rangeText(h.requirement)}`] };
}

/** Where the part is, as "Require here" states it: the centre of its body from the outline's top
 * left corner (to a tenth of a micrometre, so the saved place meets it), its turn and its face. */
export function requiredHere(part: PcbPart, f: Frame): RequiredPlace {
  const r = (v: number) => Math.round(v * 1e4) / 1e4;
  const b = part.body;
  return {
    x: r(b.x + b.width / 2 - f.originX),
    y: r(b.y + b.height / 2 - f.originY),
    rotation: r((((part.rotation % 360) + 360) % 360)),
    side: part.side,
  };
}

/** Whether "Require here" has stated the part's place: its centre on both axes. */
export function isRequired(part: PcbPart): boolean {
  return part.required.x != null && part.required.y != null;
}
