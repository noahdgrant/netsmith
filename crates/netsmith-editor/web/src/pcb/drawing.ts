// The server's drawing of the board, read for the page: its size, the nets in it, and the
// handful of changes the canvas makes to it while something is dragged.

import { UNCONNECTED, themed } from './colours';
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

/** A track at least this wide (mm) is drawn faint (`.wide`). */
export const WIDE_TRACK = 1;

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
  // The server draws the parts first and the tracks over them, so a wide track hides the pads it
  // ends on. Copper goes under the parts: pads, outlines and reference text stay readable.
  const firstPart = svg.querySelector('g[data-path]');
  if (firstPart?.parentNode) {
    for (const cls of ['l-back-copper', 'l-front-copper']) {
      const copper = svg.querySelector(`.${cls}`);
      if (copper && copper.parentNode === firstPart.parentNode) firstPart.parentNode.insertBefore(copper, firstPart);
    }
  }
  // Keepouts are hatched; the patterns live in the drawing so that every view of it (the board, the panel's preview) has them.
  const defs = doc.createElementNS('http://www.w3.org/2000/svg', 'defs');
  defs.innerHTML = [
    ['pcb-hatch-keepout', 'var(--pcb-keepout)', '.2'],
    ['pcb-hatch-limit', 'var(--pcb-height-limit)', '.16'],
  ]
    .map(
      ([id, colour, alpha]) =>
        `<pattern id="${id}" patternUnits="userSpaceOnUse" width="1.1" height="1.1" patternTransform="rotate(45)"><rect width="1.1" height="1.1" style="fill:${colour};fill-opacity:.07"/><line x1="0" y1="0" x2="0" y2="1.1" style="stroke:${colour};stroke-opacity:${alpha};stroke-width:.18"/></pattern>`,
    )
    .join('');
  svg.insertBefore(defs, svg.firstChild);
  svg.setAttribute('width', String(w * SCALE));
  svg.setAttribute('height', String(h * SCALE));
  // A track wider than a millimetre is a power track: it is drawn at its true width, and more
  // transparent, so the board under it (pads, outlines, other tracks) can still be read.
  svg.querySelectorAll('.l-front-copper line, .l-back-copper line').forEach((t) => {
    if (Number(t.getAttribute('stroke-width')) >= WIDE_TRACK) t.classList.add('wide');
  });
  const nets = new Map<string, NetItem>();
  svg.querySelectorAll('.l-pads g[data-net]').forEach((pad) => {
    const name = pad.getAttribute('data-net')!;
    const item = nets.get(name) ?? {
      name,
      colour: themed(pad.querySelector('[fill]')?.getAttribute('fill') ?? UNCONNECTED),
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
    else nets.set(name, { name, colour: themed(plane.getAttribute('stroke') ?? UNCONNECTED), pads: 0, plane: true });
  });
  // A plane is a dashed rectangle with no word on it: it is given the name of its net (a label in the drawing's own units)
  // and its place among the planes, which `pcbdraw.css` turns into the tone of its layer.
  const planes = [...svg.querySelectorAll('.l-planes > rect[data-net]')];
  planes.forEach((plane, i) => {
    plane.setAttribute('data-plane', String(i + 1));
    const label = doc.createElementNS('http://www.w3.org/2000/svg', 'text');
    label.setAttribute('class', 'plane-label');
    label.setAttribute('data-net', plane.getAttribute('data-net')!);
    label.setAttribute('data-plane', String(i + 1));
    label.setAttribute('x', String(Number(plane.getAttribute('x')) + 0.7));
    label.setAttribute('y', String(Number(plane.getAttribute('y')) + 1.25));
    label.textContent = `${plane.getAttribute('data-net')} plane`;
    plane.parentNode?.appendChild(label);
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
