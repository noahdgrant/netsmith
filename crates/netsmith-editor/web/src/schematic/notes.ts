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
    } else el.setAttribute('role', 'note');
    el.setAttribute('aria-label', noteLabel(text));
    out.push({ el, text, anchors });
  });
  return out;
}
