// The schematic's SVG, made ready for the page: sized in canvas pixels, and cut by sheet.
//
// The server draws one flat SVG (the sheets' boxes, then all parts, all wires...). The page
// regroups it so each sheet's drawing is a group of its own. Only the open sheet is shown, which
// keeps the page light on a board of many sheets, and the overview draws each sheet's group again
// at thumbnail size with `<use>`, so the whole board is not drawn once for each thumbnail.

import { type DrawingIndex, type Sheet, buildIndex, readDrawing, sheetAt } from './drawing';
import { SCALE } from './zoom';

export interface Drawing {
  html: string;
  width: number;
  height: number;
  /** The sheets, in the order the drawing lists them, in canvas pixels. */
  sheets: Sheet[];
  index: DrawingIndex;
  /** The id of the group that holds each sheet's drawing. */
  groupId: Map<string, string>;
  /** The drawing's top left, in millimetres, and the size. */
  mm: { x: number; y: number; w: number; h: number };
}

const CATEGORIES = ['parts', 'wires', 'buses', 'junctions', 'power-symbols', 'ports', 'unconnected', 'notes', 'labels'] as const;
const NS = 'http://www.w3.org/2000/svg';

const nums = (text: string | null): number[] =>
  (text ?? '').split(/[\s,]+/).filter(Boolean).map(Number).filter((n) => Number.isFinite(n));

/** A point of a drawn thing, in millimetres, to find its sheet by. */
function pointOf(e: Element): [number, number] | null {
  const own = (el: Element | null): [number, number] | null => {
    if (!el) return null;
    if (el.hasAttribute('cx')) return [Number(el.getAttribute('cx')), Number(el.getAttribute('cy'))];
    if (el.hasAttribute('x') && el.tagName.toLowerCase() === 'text') return [Number(el.getAttribute('x')), Number(el.getAttribute('y'))];
    const p = nums(el.getAttribute('points'));
    if (p.length >= 2) {
      let sx = 0,
        sy = 0,
        n = 0;
      for (let i = 0; i + 1 < p.length; i += 2) {
        sx += p[i];
        sy += p[i + 1];
        n++;
      }
      return [sx / n, sy / n];
    }
    return null;
  };
  // A note: the middle of its frame (a note with no part of its own, the module's, has no `data-ref`).
  const frame = e.querySelector('rect.note-frame');
  if (frame) return [Number(frame.getAttribute('x')) + Number(frame.getAttribute('width')) / 2, Number(frame.getAttribute('y')) + Number(frame.getAttribute('height')) / 2];
  return (
    own(e) ??
    own(e.querySelector('polyline')) ??
    own(e.querySelector('polygon')) ??
    own(e.querySelector('circle')) ??
    own(e.querySelector('text'))
  );
}

/** Make the group of each sheet and move into it what is drawn inside the sheet's box. */
function groupBySheet(svg: Element, sheets: Sheet[], origin: { x: number; y: number }, partSheet: Map<string, string>) {
  const doc = svg.ownerDocument;
  const make = (name: string, i: number) => {
    const slot = doc.createElementNS(NS, 'g');
    slot.setAttribute('class', 'sheet-slot');
    slot.setAttribute('data-sheet', name);
    const group = doc.createElementNS(NS, 'g');
    group.setAttribute('class', 'sheet-group');
    group.setAttribute('id', `nsm-sheet-${i}`);
    slot.appendChild(group);
    const cats: Record<string, Element> = {};
    for (const c of CATEGORIES) {
      const g = doc.createElementNS(NS, 'g');
      g.setAttribute('class', c);
      group.appendChild(g);
      cats[c] = g;
    }
    return { slot, group, cats };
  };
  const made = new Map<string, ReturnType<typeof make>>();
  sheets.forEach((s, i) => made.set(s.name, make(s.name, i)));
  const rest = make('', -1);
  const anchor = svg.querySelector('.sheets') ?? svg.querySelector('.parts');
  // The box and title of each sheet go first in its group.
  for (const frame of svg.querySelectorAll('.module-sheet')) {
    const m = made.get(frame.getAttribute('data-module') ?? '');
    m?.group.insertBefore(frame, m.group.firstChild);
  }
  for (const cat of CATEGORIES) {
    const holder = svg.querySelector(`:scope > g.${cat}`);
    if (!holder) continue;
    for (const child of [...holder.children]) {
      let name = '';
      const ref = child.getAttribute('data-ref');
      if (ref && partSheet.has(ref)) name = partSheet.get(ref)!;
      else {
        const p = pointOf(child);
        if (p) name = sheetAt(sheets, p[0] * SCALE - origin.x * SCALE, p[1] * SCALE - origin.y * SCALE);
      }
      (made.get(name) ?? rest).cats[cat].appendChild(child);
    }
    holder.remove();
  }
  const parent = anchor?.parentNode ?? svg;
  const before = anchor?.nextSibling ?? null;
  for (const m of made.values()) parent.insertBefore(m.slot, before);
  parent.insertBefore(rest.slot, before);
  svg.querySelector(':scope > g.sheets')?.remove();
}

/** The SVG text as a drawing, or null when it is not one. */
export function loadDrawing(text: string): Drawing | null {
  const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
  const svg = doc.documentElement;
  const bounds = svg.getAttribute('data-bounds');
  if (svg.nodeName !== 'svg' || !bounds) return null;
  const [x0, y0, w, h] = bounds.split(' ').map(Number);
  const raw = readDrawing(svg, { x: x0, y: y0 });
  const index = buildIndex(raw);
  const groupId = new Map<string, string>();
  if (raw.sheets.length > 0) {
    groupBySheet(svg, raw.sheets, { x: x0, y: y0 }, index.partSheet);
    raw.sheets.forEach((s, i) => groupId.set(s.name, `nsm-sheet-${i}`));
  }
  svg.removeAttribute('width');
  svg.removeAttribute('height');
  svg.setAttribute('width', String(w * SCALE));
  svg.setAttribute('height', String(h * SCALE));
  return {
    html: svg.outerHTML,
    width: w * SCALE,
    height: h * SCALE,
    sheets: raw.sheets,
    index,
    groupId,
    mm: { x: x0, y: y0, w, h },
  };
}
