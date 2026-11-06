// The notes of a schematic (doc comments and `==` relations, drawn by the server as `g.note`): what the page does with them.
// A note is no part and no net: a click on it opens the source where it is written (`data-at="<file>:<line>"`), through the
// same reveal as a row of the problems dock (`goTo`, which records the place so Back returns), and selects nothing.

import type { Location } from '../api';

/** Where a note is written: the file (as the project names it) and the line. */
export interface NoteAt {
  file: string;
  line: number;
}

/** `src/main.nsm:12` as a place; null for a text that is not one. The file may hold a colon (`C:/x.nsm:12`): the line is after the last. */
export function parseNoteAt(text: string | null | undefined): NoteAt | null {
  if (!text) return null;
  const colon = text.lastIndexOf(':');
  if (colon <= 0) return null;
  const digits = text.slice(colon + 1);
  if (!/^\d+$/.test(digits)) return null;
  const line = Number(digits);
  return line >= 1 ? { file: text.slice(0, colon), line } : null;
}

/** The place for `goTo`: the start of the line (the server states a line, not a column). */
export const noteLocation = (at: NoteAt): Location => ({ file: at.file, line: at.line, column: 1, endLine: at.line, endColumn: 1 });

/** The designators a note is about, from what the drawing carries. The server writes only the first part of a note (`data-ref`); a relation names more
 * (`.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance`), each as a dotted path from the instance the note belongs to (`owner`,
 * `data-note`: `""` for the board). Every dotted path whose longest prefix is a part of the board gives that part's designator, in the order the text
 * names them, after `first`. `byPath` maps a part's path from the board (`stage.led`) to its designator. A doc comment is about its first part only. */
export function noteAnchors(note: { first: string | null; owner: string; relation: boolean; text: string }, byPath: ReadonlyMap<string, string>): string[] {
  const out: string[] = [];
  const add = (r: string | undefined | null) => {
    if (r && !out.includes(r)) out.push(r);
  };
  add(note.first);
  if (!note.relation) return out;
  const base = note.owner ? `${note.owner}.` : '';
  for (const m of note.text.matchAll(/(?<![\w.)\]])\.([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)/g)) {
    const segs = m[1].split('.');
    // The longest prefix that is a part: `.stage.led.current` is the part `stage.led`.
    for (let n = segs.length; n >= 1; n--) {
      const ref = byPath.get(base + segs.slice(0, n).join('.'));
      if (ref) {
        add(ref);
        break;
      }
    }
  }
  return out;
}

/** The notes of a sheet that are lit while parts are: a note any of whose parts (`anchors`, designators) is one of `refs`. */
export function litNotes(notes: { anchors: readonly string[] }[], refs: ReadonlySet<string>): number[] {
  const out: number[] = [];
  notes.forEach((n, i) => {
    if (n.anchors.some((a) => refs.has(a))) out.push(i);
  });
  return out;
}

/** The first words of a note, for a screen reader: `Note: <first 60 characters>`, cut with an ellipsis. */
export function noteLabel(text: string): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  return `Note: ${flat.length > 60 ? `${flat.slice(0, 60).trimEnd()}…` : flat}`;
}

/** A note of the drawing, read from its DOM: the words (the tooltip holds all of them when the drawing cut them), where it is written, its parts. */
export interface NoteEl {
  el: Element;
  text: string;
  anchors: string[];
}

const SVG_NS = 'http://www.w3.org/2000/svg';
/** The focus ring of a note, in screen pixels whatever the drawing's zoom: 2 px wide, 2 px clear of the frame. */
const RING_WIDTH_PX = 2;
const RING_OFFSET_PX = 2;
const ringed = new WeakSet<Element>();

/** The ring a note shows while the keyboard has the focus on it: a crisp rectangle round the frame (not the frame's own stroke, which scales
 * with the drawing and could carry no offset), drawn by the stylesheet only on `:focus-visible`. Its size is set when the focus arrives, from
 * the zoom the drawing has then. */
function ensureRing(el: Element) {
  const frame = el.querySelector<SVGRectElement>(':scope > .note-frame');
  if (!frame) return;
  let ring = el.querySelector<SVGRectElement>(':scope > .note-ring');
  if (!ring) {
    ring = document.createElementNS(SVG_NS, 'rect') as SVGRectElement;
    ring.setAttribute('class', 'note-ring');
    ring.setAttribute('aria-hidden', 'true');
    el.appendChild(ring);
  }
  if (ringed.has(el)) return;
  ringed.add(el);
  el.addEventListener('focus', () => placeRing(el as SVGGraphicsElement, frame, ring as SVGRectElement));
}

/** The ring's rectangle for a frame, in the drawing's units, when one pixel is `scale` of them: the frame grown by half its own stroke, the offset and half the
 * ring's width, so the 2 px line stands 2 px clear of the frame's outer edge on the screen at any zoom. */
export function ringRect(frame: { x: number; y: number; width: number; height: number; rx: number; stroke?: number }, scale: number) {
  const unit = scale > 0 && Number.isFinite(scale) ? scale : 1;
  // The frame's own stroke reaches half its width beyond its rectangle (in the drawing's units): the gap is measured from the outside of that.
  const grow = (RING_OFFSET_PX + RING_WIDTH_PX / 2) / unit + Math.max(0, frame.stroke ?? 0) / 2;
  return { x: frame.x - grow, y: frame.y - grow, width: frame.width + 2 * grow, height: frame.height + 2 * grow, rx: frame.rx + grow };
}

/** Put the ring round the frame at the zoom the drawing has now (the user units one pixel is). */
function placeRing(el: SVGGraphicsElement, frame: SVGRectElement, ring: SVGRectElement) {
  const m = el.ownerSVGElement?.getScreenCTM();
  const n = (name: string) => Number(frame.getAttribute(name)) || 0;
  const stroke = parseFloat(getComputedStyle(frame).strokeWidth) || 0;
  const r = ringRect({ x: n('x'), y: n('y'), width: n('width'), height: n('height'), rx: n('rx'), stroke }, m ? Math.hypot(m.a, m.b) : 1);
  for (const [name, v] of [['x', r.x], ['y', r.y], ['width', r.width], ['height', r.height], ['rx', r.rx], ['ry', r.rx]] as const) ring.setAttribute(name, String(v));
}

/** Make the notes of the drawing reachable by keyboard (Tab, in the order the drawing writes them, which is after the parts) and readable by a screen reader,
 * and write on each the designators it is about (`data-anchors`, a space between; the page's own reading, marked `data-anchors-page`, unless the server wrote them). Called whenever the drawing or the board's detail changes; idempotent. */
export function prepareNotes(root: Element, byPath: ReadonlyMap<string, string>): NoteEl[] {
  const out: NoteEl[] = [];
  root.querySelectorAll('g.note').forEach((el) => {
    const title = el.querySelector(':scope > title')?.textContent;
    const words = title || [...el.querySelectorAll('.note-text tspan')].map((t) => t.textContent ?? '').join(' ');
    const text = words.replace(/\s+/g, ' ').trim();
    // The server's own `data-anchors` (every part, when it writes them) is kept; the page's reading of the words stands in for it.
    const served = el.hasAttribute('data-anchors') && !el.hasAttribute('data-anchors-page') ? (el.getAttribute('data-anchors') ?? '').split(' ').filter(Boolean) : null;
    const anchors = served ?? noteAnchors({ first: el.getAttribute('data-ref'), owner: el.getAttribute('data-note') ?? '', relation: el.classList.contains('note-relation'), text }, byPath);
    if (!served) {
      el.setAttribute('data-anchors', anchors.join(' '));
      el.setAttribute('data-anchors-page', '1');
    }
    // A note that says where it is written opens the source there; one that does not is only read.
    if (parseNoteAt(el.getAttribute('data-at'))) {
      el.setAttribute('tabindex', '0');
      el.setAttribute('role', 'button');
      ensureRing(el);
    } else el.setAttribute('role', 'note');
    el.setAttribute('aria-label', noteLabel(text));
    out.push({ el, text, anchors });
  });
  return out;
}
