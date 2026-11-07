// Moving a part of the schematic, as arithmetic: no DOM, so it is tested alone.
//
// A drag is measured in screen pixels and means grid steps of the drawing: the pointer's move divided by the zoom (canvas
// pixels to screen pixels) and by `SCALE` (canvas pixels to the millimetre), rounded to the 1.27 mm grid. What the file
// keeps is an offset from where the engine puts the block (ADR 0061), so a drop adds the drag's steps to the entry the
// part has; a `pin` is a place from the sheet's reference part, which is why a part that is pinned is moved by moving its pin.

import { SCALE } from './zoom';
import { type PartEntry, GRID_MM, mmToSteps, stepsToMm } from './hints';

/** A pointer that has moved less than this many screen pixels has not dragged: it is a click. */
export const DRAG_THRESHOLD = 4;

/** The grid steps a pointer's move of `dx`, `dy` screen pixels is at zoom `k` (canvas pixels to screen pixels). */
export function stepsOfMove(dx: number, dy: number, k: number): { sx: number; sy: number } {
  const per = k * SCALE * GRID_MM; // screen pixels in one step
  return { sx: Math.round(dx / per) + 0, sy: Math.round(dy / per) + 0 };
}

/** What a drag is while the pointer is down. */
export interface DragState {
  /** Whether it has gone past the threshold: below it, it may still be a click. */
  active: boolean;
  sx: number;
  sy: number;
  /** The modifier is down: a pin, not an offset. */
  pin: boolean;
}

/** The result of a finished drag: steps to move by and whether to pin, or null when nothing is to be written (a click, Esc,
 * a drag that ends where it began). */
export type DragResult = { sx: number; sy: number; pin: boolean } | null;

/** One drag, from the press to the release. */
export class DragSession {
  private x0: number;
  private y0: number;
  private state: DragState = { active: false, sx: 0, sy: 0, pin: false };
  private done = false;

  constructor(
    x: number,
    y: number,
    /** The zoom, as it was when the pointer went down (a wheel turn during a drag does not change what a step is). */
    private k: number,
  ) {
    this.x0 = x;
    this.y0 = y;
  }

  /** The pointer moved to `x`, `y` (screen pixels); `pin` is whether the modifier is down. */
  move(x: number, y: number, pin: boolean): DragState {
    if (this.done) return this.state;
    const dx = x - this.x0;
    const dy = y - this.y0;
    if (!this.state.active && Math.hypot(dx, dy) < DRAG_THRESHOLD) return (this.state = { ...this.state, pin });
    const { sx, sy } = stepsOfMove(dx, dy, this.k);
    return (this.state = { active: true, sx, sy, pin });
  }

  /** Esc: the drag is over and writes nothing. */
  cancel() {
    this.done = true;
    this.state = { active: false, sx: 0, sy: 0, pin: false };
  }

  get cancelled() {
    return this.done && !this.state.active;
  }

  get current(): DragState {
    return this.state;
  }

  /** The pointer went up. */
  finish(): DragResult {
    if (this.done) return null;
    this.done = true;
    const s = this.state;
    if (!s.active || (s.sx === 0 && s.sy === 0)) return null;
    return { sx: s.sx, sy: s.sy, pin: s.pin };
  }
}

/** What the frame of a sheet is, for a pin: where the sheet's reference part's origin was put by the engine, in millimetres of the
 * drawing, and the origin of the part that is moved. `frame` is the reference part's drawn origin less its own offset (the
 * frame starts at the place the engine puts it, not where an entry moved it). */
export interface PinFrame {
  /** The moved part's origin, as it is drawn now. */
  origin: [number, number];
  frame: [number, number];
}

/** The entry of a part after its block moved by `sx`, `sy` grid steps. `honoured` is whether what the entry says is what is
 * drawn: an entry the drawing refused is drawn at the engine's place, so a move starts from nothing. With `pin`, and a frame
 * to measure from, the block's origin is pinned where it is dropped. */
export function movedEntry(entry: PartEntry | undefined, sx: number, sy: number, pin: boolean, frame: PinFrame | null, honoured = true): PartEntry {
  const base: PartEntry = { ...(entry ?? {}) };
  const keepTurn = (e: PartEntry): PartEntry => {
    const out: PartEntry = { ...e };
    if (base.rotate !== undefined) out.rotate = base.rotate;
    if (base.mirror !== undefined) out.mirror = base.mirror;
    return out;
  };
  const dx = sx * GRID_MM;
  const dy = sy * GRID_MM;
  if (base.pin && honoured) {
    // A pinned part is moved by moving its pin.
    return keepTurn({ pin: [stepsToMm(mmToSteps(base.pin[0] + dx)), stepsToMm(mmToSteps(base.pin[1] + dy))] });
  }
  if (pin && frame) {
    const x = frame.origin[0] + dx - frame.frame[0];
    const y = frame.origin[1] + dy - frame.frame[1];
    return keepTurn({ pin: [stepsToMm(mmToSteps(x)), stepsToMm(mmToSteps(y))] });
  }
  const from = honoured ? { dx: base.dx ?? 0, dy: base.dy ?? 0 } : { dx: 0, dy: 0 };
  const nx = stepsToMm(mmToSteps(from.dx) + sx);
  const ny = stepsToMm(mmToSteps(from.dy) + sy);
  if (nx === 0 && ny === 0) return keepTurn({});
  return keepTurn({ dx: nx, dy: ny });
}

/** The orientation a drawn part has: `data-orient="<mirror 0|1> <quarter turns counterclockwise>"`. */
export interface Orientation {
  mirror: boolean;
  turns: number;
}

export function parseOrient(text: string | null | undefined): Orientation | null {
  const m = /^([01]) ([0-3])$/.exec((text ?? '').trim());
  return m ? { mirror: m[1] === '1', turns: Number(m[2]) } : null;
}

/** The entry of a part after a quarter turn counterclockwise (`dir` 1) or clockwise (-1) from the orientation it is drawn in: the
 * entry's `rotate` replaces the automatic turn, so it is written from what is drawn, not from what the file said. */
export function turnedEntry(entry: PartEntry | undefined, drawn: Orientation, dir: 1 | -1 = 1): PartEntry {
  return { ...(entry ?? {}), rotate: ((drawn.turns + dir + 4) % 4) * 90 };
}

/** The entry of a part after it is flipped left to right, from the mirror it is drawn with. */
export function mirroredEntry(entry: PartEntry | undefined, drawn: Orientation): PartEntry {
  return { ...(entry ?? {}), mirror: !drawn.mirror };
}
