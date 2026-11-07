// A plane's label sits in a corner of its plane, and a part may lie there. The label is drawn after the parts so it is never under a pad,
// which makes it lie across one instead (J2's pad, on the four-layer example). At the zoom the board is shown at, the label (a size of
// the screen, 9 to 11 px) is tried in the four corners of its plane, in the order top left, top right, bottom left, bottom right (then a line or two
// further in, under or over a label a corner has, then along the top and the bottom edge), and put in the first where it meets no part and no other label; when none is free it is hidden, and the canvas
// says so with a chip `+N` pinned to the corner of the view (`PlaneMore.tsx`: a button that lists the planes and zooms to the one chosen; a mark
// in the drawing itself was a few pixels of text over a board that was itself a few pixels). Pure geometry here (`placeAll`, `moreChip`), the page's boxes in `fitPlaneLabels`.

export type Box = { left: number; top: number; right: number; bottom: number };

/** Whether two boxes meet, `pad` apart or less. */
export function meets(a: Box, b: Box, pad = 0): boolean {
  return a.left < b.right + pad && b.left < a.right + pad && a.top < b.bottom + pad && b.top < a.bottom + pad;
}

/** The index of the first candidate box that meets none of `blocks` (`pad` apart), or -1. */
export function firstFree(candidates: Box[], blocks: Box[], pad = 2): number {
  return candidates.findIndex((c) => !blocks.some((b) => meets(c, b, pad)));
}

/** How far in from the plane's border a label sits, in the drawing's millimetres, and the line between two stacked labels. */
export const INSET = 0.9;
export const LINE = 0.7;

type Spot = { x: number; y: number; anchor: 'start' | 'end' };

const ownBox = (el: SVGTextElement, x: number, y: number, anchor: 'start' | 'middle' | 'end'): Box => {
  const m = el.getScreenCTM();
  if (!m) return boxOf(el.getBoundingClientRect());
  const fontPx = parseFloat(getComputedStyle(el).fontSize) * m.a;
  // The letters of the label, not those of a tooltip (`<title>`) it carries.
  const chars = [...el.childNodes].filter((n) => n.nodeType === 3).reduce((n, t) => n + (t.textContent ?? '').length, 0);
  return labelBox(m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f, fontPx, chars, anchor);
};

/** The places to try for a plane's label (`x y w h` in the drawing's units): each of the four corners at the first line, then at the second
 * and the third line in from the border (a label under or over the one a corner already has); the first has `stacked` lines of other
 * labels under it, as the drawing put them. */
export function spots(box: [number, number, number, number], stacked: number): Spot[] {
  const [x, y, w, h] = box;
  const out: Spot[] = [];
  for (let k = 0; k < LINES; k++) {
    out.push(
      { x: x + INSET, y: y + INSET + (k === 0 ? stacked : k) * LINE, anchor: 'start' },
      { x: x + w - INSET, y: y + INSET + k * LINE, anchor: 'end' },
      { x: x + INSET, y: y + h - INSET - k * LINE, anchor: 'start' },
      { x: x + w - INSET, y: y + h - INSET - k * LINE, anchor: 'end' },
    );
  }
  // and last, along the top and the bottom edge, every 2.5 mm: where a part or a designator leaves a gap that is not a corner
  for (let at = x + INSET + EDGE_STEP; at < x + w - INSET - EDGE_STEP; at += EDGE_STEP) {
    out.push({ x: at, y: y + INSET, anchor: 'start' }, { x: at, y: y + h - INSET, anchor: 'start' });
  }
  return out;
}

/** How many lines in from a corner's border a label is tried at, and the step of the scan along an edge, in the drawing's millimetres. */
const LINES = 3;
const EDGE_STEP = 2.5;

const boxOf = (r: DOMRect): Box => ({ left: r.left, top: r.top, right: r.right, bottom: r.bottom });

/**
 * The box a label takes on the screen, from its text and its size rather than the browser's own (an SVG text this small reports a box
 * three times as tall as its glyphs): `fontPx` screen pixels of type, `chars` characters of a bold sans (0.6 of the size each), the
 * point `(x, y)` on the screen at the middle of the first or last letter (`anchor`).
 */
export function labelBox(x: number, y: number, fontPx: number, chars: number, anchor: 'start' | 'middle' | 'end'): Box {
  const w = fontPx * 0.6 * chars;
  const h = fontPx * 1.2;
  const left = anchor === 'start' ? x : anchor === 'middle' ? x - w / 2 : x - w;
  return { left, top: y - h / 2, right: left + w, bottom: y + h / 2 };
}

/** What `placeAll` needs to know of a label: the places to try, and the box it takes at one of them (the screen's). */
export interface Placeable<L> {
  label: L;
  spots: Spot[];
  boxAt: (spot: Spot) => Box;
}

/** Where each label goes: the index of the first of its spots that meets no block and none of the labels placed before it (`pad` apart), or -1 when none does.
 * The boxes of the labels placed are returned too, in the order of the labels (null for one with no place). Labels never meet each other or a block. */
export function placeAll<L>(items: Placeable<L>[], blocks: Box[], pad = 2): { chosen: number[]; boxes: (Box | null)[] } {
  const placed: Box[] = [];
  const chosen: number[] = [];
  const boxes: (Box | null)[] = [];
  for (const item of items) {
    let at = -1;
    let box: Box | null = null;
    for (let i = 0; i < item.spots.length && at < 0; i++) {
      const b = item.boxAt(item.spots[i]);
      if (firstFree([b], [...blocks, ...placed], pad) === 0) {
        at = i;
        box = b;
        placed.push(b);
      }
    }
    chosen.push(at);
    boxes.push(box);
  }
  return { chosen, boxes };
}

/** A plane whose label has no room at this zoom: what the chip lists, and the box (the drawing's millimetres) a click zooms to. */
export interface HiddenPlane {
  /** What its label would say (`GND · In1.Cu`). */
  name: string;
  net: string;
  layer: string | null;
  /** 1 to 4: the tone of its layer, for the swatch. */
  tone: string;
  box: { x: number; y: number; width: number; height: number };
}

/** The chip for the labels that found no place: `+2`, its accessible name (the planes in a sentence) and the title it shows. Null when none was hidden. */
export function moreChip(hidden: Pick<HiddenPlane, 'name'>[]): { text: string; label: string; title: string } | null {
  if (!hidden.length) return null;
  const names = hidden.map((h) => h.name).join(', ');
  const n = hidden.length;
  return {
    text: `+${n}`,
    label: `${n} ${n === 1 ? 'plane has' : 'planes have'} no room for a label at this zoom: ${names}. Show the list`,
    title: `${n} ${n === 1 ? 'plane' : 'planes'} with no room for a label at this zoom. Click for the list`,
  };
}

/**
 * Put each plane label of the drawing in the first corner of its plane where it meets no part and no label placed before it, at the
 * zoom the drawing is at now (call it again when the zoom changes). A label with no free corner is hidden (`plane-label-off`) and
 * handed back in `planes`, for the chip that counts them. With the planes layer switched off there is nothing to label: no label is
 * hidden for want of room and `planes` is empty.
 * Returns how many were moved off their first corner, how many hidden, and which.
 */
export function fitPlaneLabels(svg: SVGSVGElement): { moved: number; hidden: number; planes: HiddenPlane[] } {
  const holder = svg.querySelector<SVGGElement>(':scope > g.l-plane-labels');
  holder?.querySelectorAll('.plane-label-more').forEach((e) => e.remove());
  const labels = [...svg.querySelectorAll<SVGTextElement>('.l-plane-labels > text.plane-label')];
  const done = { moved: 0, hidden: 0, planes: [] as HiddenPlane[] };
  if (!labels.length) return done;
  const layer = svg.querySelector('.l-planes');
  if (layer && getComputedStyle(layer).display === 'none') {
    labels.forEach((l) => l.classList.remove('plane-label-off'));
    return done;
  }
  // what a label must stay off: the parts, and the designators drawn beside them (their own boxes, not the browser's)
  const parts = [...svg.querySelectorAll('g[data-path]')].map((g) => boxOf(g.getBoundingClientRect())).filter((b) => b.right > b.left);
  for (const t of svg.querySelectorAll<SVGTextElement>('.l-refs text')) {
    parts.push(ownBox(t, Number(t.getAttribute('x')), Number(t.getAttribute('y')), (t.getAttribute('text-anchor') as 'middle' | null) ?? 'middle'));
  }
  const planes = labels.map((label) => {
    const box = (label.getAttribute('data-box') ?? '').split(' ').map(Number);
    return box.length === 4 && box.every(Number.isFinite) ? (box as [number, number, number, number]) : null;
  });
  const items: Placeable<SVGTextElement>[] = [];
  labels.forEach((label, i) => {
    const box = planes[i];
    if (!box) return;
    label.classList.remove('plane-label-off');
    items.push({
      label,
      spots: spots(box, Number(label.getAttribute('data-stack') ?? 0)),
      boxAt: (spot) => ownBox(label, spot.x, spot.y, spot.anchor),
    });
  });
  // `ownBox` reads the label as it is drawn, so each try puts it there first (the last one tried stays; a hidden one is put back at its first corner below).
  const probe = items.map((item) => ({
    ...item,
    boxAt: (spot: Spot) => {
      item.label.setAttribute('x', String(spot.x));
      item.label.setAttribute('y', String(spot.y));
      item.label.setAttribute('text-anchor', spot.anchor);
      return item.boxAt(spot);
    },
  }));
  const { chosen } = placeAll(probe, parts);
  items.forEach((item, i) => {
    const at = chosen[i];
    if (at >= 0) {
      const spot = item.spots[at];
      item.label.setAttribute('x', String(spot.x));
      item.label.setAttribute('y', String(spot.y));
      item.label.setAttribute('text-anchor', spot.anchor);
      if (at > 0) done.moved++;
      return;
    }
    const first = item.spots[0];
    item.label.setAttribute('x', String(first.x));
    item.label.setAttribute('y', String(first.y));
    item.label.setAttribute('text-anchor', first.anchor);
    item.label.classList.add('plane-label-off');
    done.hidden++;
    const box = planes[labels.indexOf(item.label)]!;
    done.planes.push({
      name: item.label.textContent ?? '',
      net: item.label.getAttribute('data-net') ?? '',
      layer: item.label.getAttribute('data-layer'),
      tone: item.label.getAttribute('data-plane') ?? '1',
      box: { x: box[0], y: box[1], width: box[2], height: box[3] },
    });
  });
  return done;
}
