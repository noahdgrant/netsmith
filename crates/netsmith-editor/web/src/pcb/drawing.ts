// The server's drawing of the board, read for the page: its size, the nets in it, and the
// handful of changes the canvas makes to it while something is dragged.

import type { Pt } from './geometry';

/** A net of the board as the drawing shows it. */
export interface NetItem {
  name: string;
  colour: string;
  pads: number;
  plane: boolean;
}

export interface Drawing {
  html: string;
  /** The canvas in pixels. */
  width: number;
  height: number;
  /** The drawing's own box: where its corner is on the sheet, in mm, and its size. */
  left: number;
  top: number;
  mmWidth: number;
  mmHeight: number;
  nets: NetItem[];
}

/** Pixels in the canvas for each millimetre of the board. */
export const SCALE = 12;

/** The drawing, and the nets in it: their names, colours and pad counts are read from it, so
 * the page needs nothing but the drawing. */
export function prepare(text: string): Drawing | null {
  const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
  const svg = doc.documentElement;
  const box = svg.getAttribute('viewBox')?.split(' ').map(Number);
  if (svg.nodeName !== 'svg' || !box) return null;
  const [left, top, w, h] = box;
  svg.setAttribute('width', String(w * SCALE));
  svg.setAttribute('height', String(h * SCALE));
  const nets = new Map<string, NetItem>();
  svg.querySelectorAll('.l-pads g[data-net]').forEach((pad) => {
    const name = pad.getAttribute('data-net')!;
    const item = nets.get(name) ?? {
      name,
      colour: pad.querySelector('[fill]')?.getAttribute('fill') ?? '#8b949e',
      pads: 0,
      plane: false,
    };
    item.pads++;
    nets.set(name, item);
  });
  svg.querySelectorAll('.l-planes > [data-net]').forEach((plane) => {
    const name = plane.getAttribute('data-net')!;
    const item = nets.get(name);
    if (item) item.plane = true;
    else nets.set(name, { name, colour: plane.getAttribute('stroke') ?? '#8b949e', pads: 0, plane: true });
  });
  return {
    html: svg.outerHTML,
    width: w * SCALE,
    height: h * SCALE,
    left,
    top,
    mmWidth: w,
    mmHeight: h,
    nets: [...nets.values()],
  };
}

/** An attribute value for a selector. */
function quoted(value: string): string {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

/** Show parts and the ratsnest lines at their ends moved by `by`, without changing the board:
 * the transforms and the line ends are put back from what was there first. */
export function previewMove(svg: SVGSVGElement, moving: { path: string; refdes: string }[], by: Pt) {
  const refs = new Set(moving.map((p) => p.refdes));
  for (const p of moving) {
    const group = svg.querySelector(`g[data-path=${quoted(p.path)}]`);
    if (!group) continue;
    const first = group.getAttribute('data-t0') ?? group.getAttribute('transform') ?? '';
    group.setAttribute('data-t0', first);
    group.setAttribute('transform', `translate(${by.x} ${by.y}) ${first}`);
  }
  svg.querySelectorAll('.l-ratsnest line').forEach((line) => {
    const from = refs.has(line.getAttribute('data-from') ?? '');
    const to = refs.has(line.getAttribute('data-to') ?? '');
    if (!from && !to) return;
    const names = ['x1', 'y1', 'x2', 'y2'];
    if (!line.hasAttribute('data-o')) line.setAttribute('data-o', names.map((n) => line.getAttribute(n)).join(' '));
    const o = line.getAttribute('data-o')!.split(' ').map(Number);
    const at = [o[0] + (from ? by.x : 0), o[1] + (from ? by.y : 0), o[2] + (to ? by.x : 0), o[3] + (to ? by.y : 0)];
    names.forEach((n, i) => line.setAttribute(n, String(at[i])));
  });
}

/** Mark the nets that are lit, and the ratsnest lines that join the parts in `joined`. With
 * no nets lit nothing is dimmed. */
export function light(svg: SVGSVGElement, nets: Set<string>, joined: Set<string>) {
  svg.classList.toggle('has-hl', nets.size > 0);
  svg.querySelectorAll('[data-net]').forEach((e) => e.classList.toggle('hl', nets.has(e.getAttribute('data-net')!)));
  svg.querySelectorAll('.l-ratsnest line').forEach((line) => {
    const on = joined.has(line.getAttribute('data-from') ?? '') || joined.has(line.getAttribute('data-to') ?? '');
    line.classList.toggle('sel-rat', on);
  });
}
