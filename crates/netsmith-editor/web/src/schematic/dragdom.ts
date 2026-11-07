// A drag in the page: the pointer events, the block that moves with them and the ghost that stays. The arithmetic is `drag.ts`;
// this file is the DOM it acts on, so it only touches the drawing's elements and one signal (the readout next to the pointer).
//
// While the pointer is down nothing is written and nothing is sent: the dragged part carries a CSS transform in the drawing's units
// (millimetres), always a whole number of grid steps, and a clone of it stays where it was. The write is the drop's.

import { signal } from '@preact/signals';

import { DragSession, type DragResult, type DragState } from './drag';
import { GRID_MM, numberText } from './hints';

/** What the readout beside the pointer says, and where (screen pixels); null when no drag is going on. */
export const dragReadout = signal<{ x: number; y: number; text: string } | null>(null);

/** The words of the readout: the offset in millimetres, and that it is a pin. */
export function readoutText(s: Pick<DragState, 'sx' | 'sy' | 'pin'>, label: string): string {
  const sign = (n: number) => (n > 0 ? `+${numberText(n)}` : numberText(n));
  const at = `dx ${sign(s.sx * GRID_MM)}  dy ${sign(s.sy * GRID_MM)} mm`;
  return `${label}  ${at}${s.pin ? '  (pin)' : ''}`;
}

export interface DragOptions {
  /** The drawing's SVG. */
  svg: Element;
  /** What the person pressed on: a part (every box of it) or a note. They carry the transform. */
  moving: Element[];
  /** The pointer's place at the press, and the zoom then. */
  x: number;
  y: number;
  k: number;
  /** What the readout calls the thing. */
  label: string;
  /** The pointer went up after a drag that moved at least a step. The click that follows is swallowed either way. */
  onDrop: (result: NonNullable<DragResult>) => void;
  /** The drag ended (dropped, cancelled or past): the owner puts things as they were. */
  onEnd?: () => void;
}

/** A clone of `el` that stays where it was: no class that selects, no attribute that names a part or a net. */
function ghostOf(el: Element): Element {
  const ghost = el.cloneNode(true) as Element;
  ghost.setAttribute('class', 'drag-ghost');
  for (const e of [ghost, ...ghost.querySelectorAll('*')]) {
    for (const a of [...e.getAttributeNames()]) if (a.startsWith('data-') || a === 'id' || a === 'tabindex' || a === 'role') e.removeAttribute(a);
  }
  ghost.setAttribute('aria-hidden', 'true');
  return ghost;
}

/** Start following the pointer. Returns a function that cancels the drag (Esc does it too). */
export function beginDrag(o: DragOptions): () => void {
  const session = new DragSession(o.x, o.y, o.k);
  let ghosts: Element[] = [];
  let began = false;
  let ended = false;

  const begin = () => {
    began = true;
    o.svg.classList.add('is-dragging');
    ghosts = o.moving.map((el) => {
      const g = ghostOf(el);
      el.parentNode?.insertBefore(g, el);
      el.classList.add('dragging');
      return g;
    });
  };
  const show = (s: DragState, e: PointerEvent) => {
    const by = `translate(${s.sx * GRID_MM}px, ${s.sy * GRID_MM}px)`;
    for (const el of o.moving) (el as SVGElement).style.transform = by;
    dragReadout.value = { x: e.clientX + 14, y: e.clientY + 16, text: readoutText(s, o.label) };
  };
  const clean = () => {
    for (const el of o.moving) {
      el.classList.remove('dragging');
      (el as SVGElement).style.transform = '';
    }
    for (const g of ghosts) g.remove();
    ghosts = [];
    o.svg.classList.remove('is-dragging');
    dragReadout.value = null;
  };
  const end = () => {
    if (ended) return;
    ended = true;
    window.removeEventListener('pointermove', move, true);
    window.removeEventListener('pointerup', up, true);
    window.removeEventListener('pointercancel', abort, true);
    window.removeEventListener('keydown', key, true);
    if (began) clean();
    o.onEnd?.();
  };
  const swallowClick = () => {
    const stop = (ev: Event) => ev.stopPropagation();
    window.addEventListener('click', stop, { capture: true, once: true });
    setTimeout(() => window.removeEventListener('click', stop, { capture: true }), 0);
  };
  const move = (e: PointerEvent) => {
    const s = session.move(e.clientX, e.clientY, e.altKey);
    if (!s.active) return;
    if (!began) begin();
    show(s, e);
  };
  const up = (e: PointerEvent) => {
    // The modifier is read at the drop too, so it can be pressed or let go just before.
    session.move(e.clientX, e.clientY, e.altKey);
    const result = session.finish();
    if (began) swallowClick();
    end();
    if (result) o.onDrop(result);
  };
  const abort = () => {
    session.cancel();
    end();
  };
  const key = (e: KeyboardEvent) => {
    if (e.key !== 'Escape') return;
    // Esc is the drag's: it does not also clear the selection.
    e.preventDefault();
    e.stopPropagation();
    abort();
  };
  window.addEventListener('pointermove', move, true);
  window.addEventListener('pointerup', up, true);
  window.addEventListener('pointercancel', abort, true);
  window.addEventListener('keydown', key, true);
  return abort;
}
