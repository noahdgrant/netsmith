// The edits of the PCB canvas as changes to the layout file (`PcbChange`), and the history that
// undoes them and the edits of the routes file. A change says what an entry was and what it
// becomes, so the inverse of an edit is the edit turned round, and the server refuses one that is
// not what the file has now.

import type { KeepoutPlacement, PartPlacement, PcbBox, PcbChange, PcbPart, RoutesChange, RoutesUndo } from '../api';
import { type Pt, centre, normAngle, rotateAbout, tidy, unite } from './geometry';

/** Where the layout file's coordinates start on the sheet: the outline's top left corner. */
export interface Frame {
  originX: number;
  originY: number;
}

/** The part as the layout file would have it. */
export function placementOf(p: PcbPart, f: Frame): PartPlacement {
  return {
    x: tidy(p.x - f.originX),
    y: tidy(p.y - f.originY),
    rotation: tidy(p.rotation),
    side: p.side,
    locked: p.locked,
    source: p.placement === 'derived' ? null : p.placement,
  };
}

/** The part's entry in the file now, or `null` while the placer places it. */
export function entryOf(p: PcbPart, f: Frame): PartPlacement | null {
  return p.placement === 'derived' ? null : placementOf(p, f);
}

function change(p: PcbPart, f: Frame, after: Partial<PartPlacement> | null): PcbChange {
  return {
    what: 'part',
    path: p.path,
    before: entryOf(p, f),
    after: after === null ? null : { ...placementOf(p, f), ...after, source: 'hand' },
  };
}

/** Move parts by `by`. A locked part stays where it is. */
export function moveParts(parts: PcbPart[], f: Frame, by: Pt): PcbChange[] {
  if (by.x === 0 && by.y === 0) return [];
  return parts
    .filter((p) => !p.locked)
    .map((p) => {
      const at = placementOf(p, f);
      return change(p, f, { x: tidy(at.x + by.x), y: tidy(at.y + by.y) });
    });
}

/** The middle of the parts' courtyards, in the layout file's coordinates. */
function middle(parts: PcbPart[], f: Frame): Pt {
  const c = centre(unite(parts.map((p) => p.courtyard))!);
  return { x: c.x - f.originX, y: c.y - f.originY };
}

/** Turn parts by `degrees` counter-clockwise: each about its own origin when it is alone, else
 * all about the middle of the selection. A locked part stays. */
export function rotateParts(parts: PcbPart[], f: Frame, degrees: number): PcbChange[] {
  const free = parts.filter((p) => !p.locked);
  if (free.length === 0) return [];
  const about = free.length > 1 ? middle(free, f) : null;
  return free.map((p) => {
    const at = placementOf(p, f);
    const to = about ? rotateAbout(at, about, degrees) : at;
    return change(p, f, { x: tidy(to.x), y: tidy(to.y), rotation: normAngle(tidy(at.rotation + degrees)) });
  });
}

/** Put parts on the other face. A footprint is mirrored left to right, as KiCad does it, so
 * its turn is the negative of what it was; a selection is mirrored about its middle. */
export function flipParts(parts: PcbPart[], f: Frame): PcbChange[] {
  const free = parts.filter((p) => !p.locked);
  if (free.length === 0) return [];
  const axis = free.length > 1 ? middle(free, f).x : null;
  return free.map((p) => {
    const at = placementOf(p, f);
    return change(p, f, {
      x: axis === null ? at.x : tidy(2 * axis - at.x),
      rotation: normAngle(tidy(-at.rotation)),
      side: p.side === 'top' ? 'bottom' : 'top',
    });
  });
}

/** Lock or unlock parts where they are. */
export function lockParts(parts: PcbPart[], f: Frame, lock: boolean): PcbChange[] {
  return parts.filter((p) => p.locked !== lock).map((p) => change(p, f, { locked: lock }));
}

/** Hand the parts back to the placer: their entries go. */
export function releaseParts(parts: PcbPart[], f: Frame): PcbChange[] {
  return parts.filter((p) => p.placement !== 'derived' && !p.locked).map((p) => change(p, f, null));
}

/** The change that undoes `changes`: the same entries in the opposite order, turned round. */
export function invert(changes: PcbChange[]): PcbChange[] {
  return [...changes].reverse().map((c) => ({ ...c, before: c.after, after: c.before }) as PcbChange);
}

/** A keepout drawn on the board: the box is on the sheet. */
export function drawKeepout(
  name: string,
  box: PcbBox,
  f: Frame,
  what: { layers: string[]; tracks: boolean; vias: boolean; copper: boolean; parts: boolean },
): PcbChange {
  return {
    what: 'keepout',
    name,
    before: null,
    after: {
      x: tidy(box.x - f.originX),
      y: tidy(box.y - f.originY),
      drawn: { width: tidy(box.width), height: tidy(box.height), ...what },
    },
  };
}

/** What the file has of a keepout, as the scene shows it. */
export function keepoutEntry(
  k: { area: PcbBox | null; layers: string[]; tracks: boolean; vias: boolean; copper: boolean; parts: boolean; origin: string },
  f: Frame,
): KeepoutPlacement | null {
  if (!k.area || (k.origin !== 'drawn' && k.origin !== 'placed')) return null;
  const at = { x: tidy(k.area.x - f.originX), y: tidy(k.area.y - f.originY) };
  if (k.origin === 'placed') return { ...at, drawn: null };
  return {
    ...at,
    drawn: {
      width: tidy(k.area.width),
      height: tidy(k.area.height),
      layers: k.layers,
      tracks: k.tracks,
      vias: k.vias,
      copper: k.copper,
      parts: k.parts,
    },
  };
}

/** One step of undo: what the user did, as the changes to the layout file that did it, or as the
 * records of the routes file it changed (each net's record before and after, as the server gave
 * them: a delete renumbers the shapes after it, so the inverse is the records put back, not a
 * change turned round). An edit that shoved the copper of other nets (issue 726) changed several
 * records: one step puts them all back, and redoes them all. */
export type Step = { label: string; changes: PcbChange[] } | { label: string; routes: RoutesUndo[] };

/** What undoing or redoing a step sends: changes to the layout file, or records of the routes file put back (one edit). */
export type Run = { layout: PcbChange[] } | { routes: RoutesChange[] };

export interface History {
  undo: Step[];
  redo: Step[];
}

export const NO_HISTORY: History = { undo: [], redo: [] };

const LIMIT = 200;

/** A step done: it can be undone, and what was undone before can no longer be redone. */
export function record(h: History, step: Step): History {
  return { undo: [...h.undo, step].slice(-LIMIT), redo: [] };
}

/** The change that puts a net's record back as it was before the edit (`back`), or as the edit left it. */
export function restore(u: RoutesUndo, back: boolean): RoutesChange {
  return { what: 'restore', net: u.net, before: back ? u.after : u.before, after: back ? u.before : u.after };
}

function runOf(step: Step, back: boolean): Run {
  if ('routes' in step) return { routes: step.routes.map((u) => restore(u, back)) };
  return { layout: back ? invert(step.changes) : step.changes };
}

/** The step to undo, what undoes it, and the history after it is. */
export function undoing(h: History): { step: Step; run: Run; next: History } | null {
  const step = h.undo[h.undo.length - 1];
  if (!step) return null;
  return { step, run: runOf(step, true), next: { undo: h.undo.slice(0, -1), redo: [...h.redo, step] } };
}

export function redoing(h: History): { step: Step; run: Run; next: History } | null {
  const step = h.redo[h.redo.length - 1];
  if (!step) return null;
  return { step, run: runOf(step, false), next: { undo: [...h.undo, step], redo: h.redo.slice(0, -1) } };
}
