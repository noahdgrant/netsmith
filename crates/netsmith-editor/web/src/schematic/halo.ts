// The overlay a part of the schematic wears when it is picked, hovered, linked from the source or marked by the
// comparison: a soft rounded rectangle around its body, drawn behind it. The symbol's own strokes are never
// recoloured by those states (a recoloured or dashed symbol reads as a broken one); the page's CSS paints the
// rectangle by the class the part has (`mark`, `linked`, `hover-lit`, `cmp-*`).
//
// The rectangles are the part's previous siblings, not its children: a part's box (what the recipes measure, what a
// click in the gap around a symbol hits) is the symbol's, and a rectangle that is a child would grow it. The CSS finds
// the part by `:has(+ .part)`, which is why the order matters: `.part-cmp`, `.part-halo`, the part.
//
// The rectangles are there from the first look at a sheet, empty (no fill, no stroke), so a state is a class and
// nothing is added or taken away while the pointer moves. They are made once a part has a box (a sheet that is not
// on show has none), from the boxes of the body's own shapes and pins, not of the labels.

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The smallest box holding every box; null for none. Empty boxes (a text with no size, a hidden shape) are left out. */
export function unionBoxes(boxes: Box[]): Box | null {
  const real = boxes.filter((b) => b.width > 0 || b.height > 0);
  if (real.length === 0) return null;
  const x0 = Math.min(...real.map((b) => b.x));
  const y0 = Math.min(...real.map((b) => b.y));
  const x1 = Math.max(...real.map((b) => b.x + b.width));
  const y1 = Math.max(...real.map((b) => b.y + b.height));
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** A box grown by `pad` on every side, and never smaller than `least` across (a resistor's body is a thin bar). */
export function padBox(b: Box, pad: number, least = 0): Box {
  const w = Math.max(b.width + 2 * pad, least);
  const h = Math.max(b.height + 2 * pad, least);
  return { x: b.x + b.width / 2 - w / 2, y: b.y + b.height / 2 - h / 2, width: w, height: h };
}

/** Millimetres of the drawing around the body: the halo of a pick, and the comparison's mark outside it. */
export const HALO_PAD = 1.1;
export const MARK_PAD = 2.2;
/** The dashed outline of a part that has an entry in the layout file (ADR 0061) stands outside the comparison's mark. */
export const HINT_PAD = 3.4;

const NS = 'http://www.w3.org/2000/svg';
const BODY = ':scope > :is(polyline, polygon, rect, circle, path, ellipse, line), :scope > g.pin > line.pin-line';

function bodyBox(part: SVGGElement): Box | null {
  const boxes: Box[] = [];
  for (const e of part.querySelectorAll<SVGGraphicsElement>(BODY)) {
    // A shape with a transform of its own has its box in another space: the group's whole box is the answer then.
    if (e.hasAttribute('transform') || e.closest('g.pin')?.hasAttribute('transform')) return unionBoxes([part.getBBox()]);
    const b = e.getBBox();
    boxes.push({ x: b.x, y: b.y, width: b.width, height: b.height });
  }
  return unionBoxes(boxes);
}

function rect(cls: string, b: Box, rx: number): SVGRectElement {
  const r = document.createElementNS(NS, 'rect');
  r.setAttribute('class', cls);
  r.setAttribute('x', String(+b.x.toFixed(3)));
  r.setAttribute('y', String(+b.y.toFixed(3)));
  r.setAttribute('width', String(+b.width.toFixed(3)));
  r.setAttribute('height', String(+b.height.toFixed(3)));
  r.setAttribute('rx', String(rx));
  r.setAttribute('aria-hidden', 'true');
  return r;
}

/** The rectangle that marks a part as arranged by hand (the layout file has an entry for it), or null for a part with no halos yet. It is
 * the third of the part's previous siblings: `part-hint`, `part-cmp`, `part-halo`, the part. */
export function hintRectOf(part: Element): Element | null {
  const r = part.previousElementSibling?.previousElementSibling?.previousElementSibling;
  return r?.classList.contains('part-hint') ? r : null;
}

/** Give every part of `root` that has a box and no halo yet its three rectangles, in front of it in the document (`part-hint` outside
 * `part-cmp` outside `part-halo`). */
export function addHalos(root: Element) {
  for (const part of root.querySelectorAll<SVGGElement>('g.part:not([data-halo])')) {
    // Nothing is laid out for a part of a sheet that is hidden: it is done when the sheet is shown.
    if (part.getClientRects().length === 0) continue;
    const body = bodyBox(part);
    if (!body) continue;
    const parent = part.parentNode;
    if (!parent) continue;
    parent.insertBefore(rect('part-hint', padBox(body, HINT_PAD, 2 * HINT_PAD), 2.4), part);
    parent.insertBefore(rect('part-cmp', padBox(body, MARK_PAD, 2 * MARK_PAD), 1.8), part);
    parent.insertBefore(rect('part-halo', padBox(body, HALO_PAD, 2 * HALO_PAD), 1.2), part);
    part.setAttribute('data-halo', '');
  }
}
