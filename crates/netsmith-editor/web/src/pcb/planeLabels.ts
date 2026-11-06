// A plane's label sits in a corner of its plane, and a part may lie there. The label is drawn after the parts so it is never under a pad,
// which makes it lie across one instead (J2's pad, on the four-layer example). At the zoom the board is shown at, the label (a size of
// the screen, 9 to 11 px) is tried in the four corners of its plane, in the order top left, top right, bottom left, bottom right (then a line or two
// further in, under or over a label a corner has, then along the top and the bottom edge), and put in the first where it meets no part and no other label; when none is free it is hidden (zooming in makes room; the plane is named
// in the net list and the layers panel as well). Pure geometry here, the page's boxes in `fitPlaneLabels`.

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
  return labelBox(m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f, fontPx, (el.textContent ?? '').length, anchor);
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

/**
 * Put each plane label of the drawing in the first corner of its plane where it meets no part and no label placed before it, at the
 * zoom the drawing is at now (call it again when the zoom changes). A label with no free corner is hidden (`plane-label-off`).
 * Returns how many were moved off their first corner, and how many hidden.
 */
export function fitPlaneLabels(svg: SVGSVGElement): { moved: number; hidden: number } {
  const labels = [...svg.querySelectorAll<SVGTextElement>('.l-plane-labels > text.plane-label')];
  const done = { moved: 0, hidden: 0 };
  if (!labels.length) return done;
  // what a label must stay off: the parts, and the designators drawn beside them (their own boxes, not the browser's)
  const parts = [...svg.querySelectorAll('g[data-path]')].map((g) => boxOf(g.getBoundingClientRect())).filter((b) => b.right > b.left);
  for (const t of svg.querySelectorAll<SVGTextElement>('.l-refs text')) {
    parts.push(ownBox(t, Number(t.getAttribute('x')), Number(t.getAttribute('y')), (t.getAttribute('text-anchor') as 'middle' | null) ?? 'middle'));
  }
  const placed: Box[] = [];
  for (const label of labels) {
    const box = (label.getAttribute('data-box') ?? '').split(' ').map(Number);
    if (box.length !== 4 || box.some((n) => !Number.isFinite(n))) continue;
    const corners = spots(box as [number, number, number, number], Number(label.getAttribute('data-stack') ?? 0));
    label.classList.remove('plane-label-off');
    let chosen = -1;
    for (let i = 0; i < corners.length && chosen < 0; i++) {
      label.setAttribute('x', String(corners[i].x));
      label.setAttribute('y', String(corners[i].y));
      label.setAttribute('text-anchor', corners[i].anchor);
      const at = ownBox(label, corners[i].x, corners[i].y, corners[i].anchor);
      if (firstFree([at], [...parts, ...placed]) === 0) {
        chosen = i;
        placed.push(at);
      }
    }
    if (chosen < 0) {
      label.setAttribute('x', String(corners[0].x));
      label.setAttribute('y', String(corners[0].y));
      label.setAttribute('text-anchor', corners[0].anchor);
      label.classList.add('plane-label-off');
      done.hidden++;
    } else if (chosen > 0) done.moved++;
  }
  return done;
}
