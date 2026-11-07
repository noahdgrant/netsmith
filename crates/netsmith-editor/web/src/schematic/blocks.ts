// What the page needs to know about a part to write its entry in the layout file (ADR 0061), read from the drawing: where its
// symbol's origin is, how it is turned, whether it is the reference part of its sheet, and which sheet of the file it belongs to.
// `readFacts` reads the SVG's `data-origin`, `data-orient` and `data-reference`; the rest is arithmetic on those.

import { type Orientation, type PinFrame, parseOrient } from './drag';
import { type HintAddress, type PartEntry, addressOf, keyFrom } from './hints';

/** One drawn box of a part (a part with several units has a box for each; the first in the drawing is its primary unit's). */
export interface PartFacts {
  ref: string;
  /** The path from the board (`stage.filter.l1`), from the part list. */
  path: string;
  address: HintAddress;
  orient: Orientation | null;
  /** The origin of the symbol as drawn, in millimetres. */
  origin: [number, number] | null;
  /** The reference part of its sheet: the one the sheet's frame starts at, which is inert. */
  reference: boolean;
  /** The drawn sheet (`stage`, `stage (2/3)`, or the empty name when the board is one picture): a cut sheet has a reference part for each box. */
  slot: string;
}

const pair = (text: string | null): [number, number] | null => {
  const m = (text ?? '').trim().split(/\s+/).map(Number);
  return m.length === 2 && m.every(Number.isFinite) ? [m[0], m[1]] : null;
};

/** The facts of every part drawn in `root` (the SVG), by designator; a part's first box in the drawing is its primary unit. `pathOf`
 * is the part list's path of a designator. */
export function readFacts(root: ParentNode, pathOf: (ref: string) => string | undefined): Map<string, PartFacts> {
  const out = new Map<string, PartFacts>();
  for (const g of root.querySelectorAll('g.part[data-ref]')) {
    const ref = g.getAttribute('data-ref')!;
    const path = pathOf(ref);
    if (path === undefined) continue;
    const slot = g.closest('.sheet-slot')?.getAttribute('data-sheet') ?? '';
    const first = out.get(ref);
    if (first) {
      // A further unit's box: the reference part is the box the engine marked; the part is the reference if any of its boxes is.
      if (g.hasAttribute('data-reference')) first.reference = true;
      continue;
    }
    out.set(ref, {
      ref,
      path,
      address: addressOf(path),
      orient: parseOrient(g.getAttribute('data-orient')),
      origin: pair(g.getAttribute('data-origin')),
      reference: g.hasAttribute('data-reference'),
      slot,
    });
  }
  return out;
}

/** The sheet of the file a drawn sheet is: `stage (2/3)` is `stage`; a name that is not a module instance of the board (the board's
 * own box, named for the board, or none) is the board's own sheet, `null`. */
export function hintSheet(slot: string, modules: ReadonlySet<string>): string | null {
  const name = slot.replace(/ \(\d+\/\d+\)$/, '');
  return modules.has(name) ? name : null;
}

/** The module instances of the board: the first word of every path that has more than one. */
export function moduleNames(paths: Iterable<string>): Set<string> {
  const out = new Set<string>();
  for (const p of paths) {
    const at = p.indexOf('.');
    if (at > 0) out.add(p.slice(0, at));
  }
  return out;
}

/** The reference part of the sheet `part` is on, if the drawing marks one. */
export function referenceOf(facts: Map<string, PartFacts>, part: PartFacts): PartFacts | undefined {
  for (const f of facts.values()) {
    if (f.reference && f.slot === part.slot && f.address.sheet === part.address.sheet) return f;
  }
  return undefined;
}

/** What a `pin` is measured from for `part`: the origin of the sheet's frame, which is where the engine puts the reference part, not
 * where an entry of its own moved it (`entryOf` says the entry a part has and whether the drawing keeps it). */
export function pinFrame(
  facts: Map<string, PartFacts>,
  part: PartFacts,
  entryOf: (f: PartFacts) => { entry: PartEntry | undefined; honoured: boolean },
): PinFrame | null {
  const ref = referenceOf(facts, part);
  if (!ref?.origin || !part.origin) return null;
  const { entry: e, honoured } = entryOf(ref);
  const off = e && honoured && !e.pin ? [e.dx ?? 0, e.dy ?? 0] : [0, 0];
  return { origin: part.origin, frame: [ref.origin[0] - off[0], ref.origin[1] - off[1]] };
}

/** A note's address from the drawing: `g.note[data-note, data-n]`; `nth` counts, from 1, the notes of the same path and kind in
 * the order of `data-n` (the design's own order, as the file counts). `slot` is the sheet it is drawn on. */
export interface NoteFacts {
  index: number;
  path: string;
  /** The designator of the first part the note is about, if it names one. */
  ref: string | null;
  kind: 'doc' | 'relation';
  nth: number;
  slot: string;
}

export function readNotes(root: ParentNode): Map<number, NoteFacts> {
  const raw = [...root.querySelectorAll('g.note[data-note][data-n]')].map((g) => ({
    index: Number(g.getAttribute('data-n')),
    path: g.getAttribute('data-note') ?? '',
    ref: g.getAttribute('data-ref'),
    kind: g.classList.contains('note-relation') ? ('relation' as const) : ('doc' as const),
    slot: g.closest('.sheet-slot')?.getAttribute('data-sheet') ?? '',
  }));
  raw.sort((a, b) => a.index - b.index);
  const counts = new Map<string, number>();
  const out = new Map<number, NoteFacts>();
  for (const n of raw) {
    const key = `${n.path}\u0000${n.kind}`;
    const nth = (counts.get(key) ?? 0) + 1;
    counts.set(key, nth);
    out.set(n.index, { ...n, nth });
  }
  return out;
}

/** The key a note has in the file, given the sheet it is drawn on. */
export const noteKeyOf = (n: NoteFacts, sheet: string | null): string => `${keyFrom(sheet, n.path)}#${n.kind}${n.kind === 'relation' && n.nth > 1 ? n.nth : ''}`;

/** The sheet of the file a note is on: the drawn sheet's, or, for a board drawn as one picture (no sheets in the drawing), the sheet of the part
 * it is about, else the module its path is in. `null` is the board's own sheet. */
export function noteSheet(note: NoteFacts, facts: Map<string, PartFacts>, modules: ReadonlySet<string>): string | null {
  if (note.slot !== '') return hintSheet(note.slot, modules);
  const anchored = note.ref ? facts.get(note.ref)?.address.sheet : undefined;
  if (anchored !== undefined) return anchored;
  const first = note.path.split('.')[0];
  return modules.has(first) ? first : null;
}
