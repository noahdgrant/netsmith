// Moving the picked parts in the 3D view (issue 772). A drag that starts on a picked part moves
// the selection in the board's plane as a drag in the 2D view does: the part taken ends on the
// grid, a part too tall for a height limit stops at its edge (ADR 0045 section 11), and the drop
// writes the layout file through the same edit (`moveParts`). A drag anywhere else orbits. The
// keys R, F, L and the arrows edit the picked parts as they do in 2D. Plain numbers, no three.js,
// so it is tested on its own.

import type { PcbPart, PcbScene } from '../api';
import { type Pt, snapMove } from '../pcb/geometry';
import { type Held, type Zone, limitMove, zonesOf } from '../pcb/limits';

/** A drag of the picked parts, from its start. */
export interface Move3d {
  /** The parts that move: the selection's, less the locked ones. */
  free: PcbPart[];
  /** Every part of the selection, locked or not (to say why nothing moved). */
  parts: PcbPart[];
  /** The part taken, in the layout file's frame: it ends on the grid and the others keep their distance to it. */
  anchor: Pt;
  /** The height limits of the board (not those a part carries, which move with their part). */
  zones: Zone[];
  /** Where the drag started, in the scene's frame (y grows toward the board's top edge). */
  start: Pt;
  /** The last move shown, on the sheet: a height limit is walked from there. */
  last: Pt;
}

/** The drag that a press on the part `hit` at `start` (the scene's frame) begins, or null when the
 * part is not picked or not on the board: then the press orbits. */
export function startMove(sc: PcbScene, selected: string[], hit: string, start: Pt): Move3d | null {
  if (!selected.includes(hit)) return null;
  const parts = sc.parts.filter((p) => selected.includes(p.path));
  const taken = parts.find((p) => p.path === hit);
  if (!taken) return null;
  const free = parts.filter((p) => !p.locked);
  const first = free.includes(taken) ? taken : (free[0] ?? taken);
  return {
    free,
    parts,
    anchor: { x: first.x - sc.originX, y: first.y - sc.originY },
    zones: zonesOf(sc.keepouts.filter((k) => k.origin !== 'part')),
    start,
    last: { x: 0, y: 0 },
  };
}

/** How far the parts move on the sheet when the pointer is at `now` (the scene's frame), on a grid
 * of `step` (0 is none), and the limit that held them, if one did. */
export function moveTo(m: Move3d, now: Pt, step: number): { delta: Pt; held: Held | null } {
  // The scene's y grows up the board, the sheet's down.
  const by = { x: now.x - m.start.x, y: m.start.y - now.y };
  const limited = limitMove(m.free, m.zones, snapMove(m.anchor, by, step), m.last, step);
  m.last = limited.delta;
  return limited;
}

/** A move on the sheet as the same move in the scene's frame. */
export const toSceneOffset = (d: Pt): Pt => ({ x: d.x, y: 0 - d.y });

/** What a key does to the picked parts in 3D, as in 2D; null when it is the view's own key (with
 * nothing picked, F fits and the arrows orbit). The arrows nudge along the board's axes, as in 2D,
 * whichever way the camera looks. */
export type KeyEdit = { kind: 'rotate'; degrees: number } | { kind: 'flip' } | { kind: 'lock' } | { kind: 'nudge'; dx: number; dy: number; big: boolean };

export function editKey(key: string, shift: boolean, picked: boolean): KeyEdit | null {
  if (!picked) return null;
  const k = key.toLowerCase();
  if (k === 'r') return { kind: 'rotate', degrees: shift ? -90 : 90 };
  // Shift+F turns the board over (a command of the view).
  if (k === 'f' && !shift) return { kind: 'flip' };
  if (k === 'l' && !shift) return { kind: 'lock' };
  if (k.startsWith('arrow')) {
    const dx = k === 'arrowleft' ? -1 : k === 'arrowright' ? 1 : 0;
    const dy = k === 'arrowup' ? -1 : k === 'arrowdown' ? 1 : 0;
    return dx || dy ? { kind: 'nudge', dx, dy, big: shift } : null;
  }
  return null;
}
