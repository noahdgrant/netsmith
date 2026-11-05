// The one selection of the page. A part, a net or a module is picked in any view (the schematic,
// the PCB, the power tree, the system view, the bill of materials, the source's tree) and every
// view shows it. A view reads `selection` and writes it with `select`; it keeps no choice of its own.
//
// The key is what survives an edit: the instance path for a part and a module (`stage1.regulator`,
// `stage1`), the name for a net (`RAIL1`). Never the designator, whose letters and numbers change
// when a part is added. The functions here are pure (they take the board's detail as an argument)
// so that they can be tested without a page.

import { signal } from '@preact/signals';

import type { BoardDetail, Location, PartInfo } from './api';

export type SelectionKind = 'part' | 'net' | 'module';

export interface Selection {
  kind: SelectionKind;
  /** The instance path of a part or a module, the name of a net. */
  key: string;
}

/** The thing picked, or null. */
export const selection = signal<Selection | null>(null);

/** A request to bring the selection into view in the views that show it (go to anything, a click
 * on the status chip). `nonce` makes the same selection asked for twice a new request. */
export const revealRequest = signal<{ sel: Selection; nonce: number } | null>(null);
let nonce = 0;

export const sameSelection = (a: Selection | null, b: Selection | null): boolean =>
  a === b || (!!a && !!b && a.kind === b.kind && a.key === b.key);

/** Pick `sel` in every view; null clears. Picking what is picked already changes nothing. */
export function setSelection(sel: Selection | null) {
  if (!sameSelection(selection.peek(), sel)) selection.value = sel;
}

/** Pick `sel` and ask the views to bring it into view. */
export function selectAndReveal(sel: Selection) {
  setSelection(sel);
  revealRequest.value = { sel, nonce: ++nonce };
}

export const partSel = (path: string): Selection => ({ kind: 'part', key: path });
export const netSel = (name: string): Selection => ({ kind: 'net', key: name });
export const moduleSel = (path: string): Selection => ({ kind: 'module', key: path });

/** Whether instance path `path` is `ancestor` or below it (`a.b.c` is under `a.b`, not under `a.bc`). */
export const under = (path: string, ancestor: string): boolean => path === ancestor || path.startsWith(`${ancestor}.`);

/** The modules of a board: every instance path above a part (`a.b.c` gives `a` and `a.b`), with the
 * parts under each, sorted by path. */
export interface ModuleInfo {
  path: string;
  /** The parts under it, at any depth. */
  parts: PartInfo[];
  /** The declaration of the instance (`.stage: LdoStage`), when a part of it records it. */
  decl: Location | null;
}

export function modulesOf(detail: BoardDetail | null): ModuleInfo[] {
  const map = new Map<string, ModuleInfo>();
  for (const part of detail?.parts ?? []) {
    const segs = part.path.split('.');
    for (let depth = 1; depth < segs.length; depth++) {
      const path = segs.slice(0, depth).join('.');
      let m = map.get(path);
      if (!m) map.set(path, (m = { path, parts: [], decl: null }));
      m.parts.push(part);
      // `scopes` is the declarations of what contains the part, innermost first: the part `a.b.c`
      // has [`.b` of `a`, `.a`]. The one for depth `d` is `d` places from the end.
      const scope = part.scopes[segs.length - 1 - depth];
      if (scope && !m.decl && part.scopes.length === segs.length - 1) m.decl = scope;
    }
  }
  return [...map.values()].sort((a, b) => a.path.localeCompare(b.path, undefined, { numeric: true }));
}

/** The part that has instance path `path`. */
export const partByPath = (d: BoardDetail | null, path: string): PartInfo | undefined => d?.parts.find((p) => p.path === path);

/** The part with designator `refdes`, for the places that only know the letters (the bill of materials). */
export const partByRefdes = (d: BoardDetail | null, refdes: string): PartInfo | undefined => d?.parts.find((p) => p.refdes === refdes);

/** The designators the selection stands for: the part itself, the parts on the net, the parts
 * under the module. A view that lists designators (the bill of materials) lights these. */
export function refdesOf(d: BoardDetail | null, sel: Selection | null): Set<string> {
  const out = new Set<string>();
  if (!d || !sel) return out;
  if (sel.kind === 'part') {
    const p = partByPath(d, sel.key);
    if (p) out.add(p.refdes);
  } else if (sel.kind === 'net') {
    for (const pin of d.nets.find((n) => n.name === sel.key)?.pins ?? []) out.add(pin.refdes);
  } else {
    for (const p of d.parts) if (under(p.path, sel.key)) out.add(p.refdes);
  }
  return out;
}

/** The paths of the parts the selection stands for (a net gives none: it lights, it does not own). */
export function partPathsOf(d: BoardDetail | null, sel: Selection | null): string[] {
  if (!d || !sel) return [];
  if (sel.kind === 'part') return partByPath(d, sel.key) ? [sel.key] : [];
  if (sel.kind === 'module') return d.parts.filter((p) => under(p.path, sel.key)).map((p) => p.path);
  return [];
}

/** Whether the board still has what the selection names. Nothing is known before the board is read. */
export function exists(d: BoardDetail | null, sel: Selection): boolean {
  if (!d) return true;
  if (sel.kind === 'part') return !!partByPath(d, sel.key);
  if (sel.kind === 'net') return d.nets.some((n) => n.name === sel.key);
  return d.parts.some((p) => p.path.startsWith(`${sel.key}.`));
}

/** What a thing is called for a person: the title and the line under it. */
export function describe(d: BoardDetail | null, sel: Selection): { title: string; sub: string } {
  if (sel.kind === 'part') {
    const p = partByPath(d, sel.key);
    return p
      ? { title: `${p.refdes} ${p.value}`.trim(), sub: `${p.kind} · ${p.path}` }
      : { title: sel.key, sub: 'part' };
  }
  if (sel.kind === 'net') {
    const n = d?.nets.find((x) => x.name === sel.key);
    return { title: sel.key, sub: n ? `net · ${n.pins.length} pin${n.pins.length === 1 ? '' : 's'}` : 'net' };
  }
  const m = modulesOf(d).find((x) => x.path === sel.key);
  return { title: sel.key, sub: m ? `module · ${m.parts.length} part${m.parts.length === 1 ? '' : 's'}` : 'module' };
}

/** Where the selection is declared, if it has a place in the source (a net has none: only the board names it). */
export function declarationOf(d: BoardDetail | null, sel: Selection): Location | null {
  if (sel.kind === 'part') return partByPath(d, sel.key)?.decl ?? null;
  if (sel.kind === 'module') return modulesOf(d).find((m) => m.path === sel.key)?.decl ?? null;
  return null;
}

/** A card of a diagram (a rail, a converter, a module of the system) and what it stands for. */
export interface Card {
  id: string;
  kind: SelectionKind;
  key: string;
}

/** The card that shows `sel`: an exact match, else for a part or a module the card of the nearest
 * module above it (a part of a converter selects the converter). Null when no card does. */
export function cardFor(sel: Selection | null, cards: Card[]): string | null {
  if (!sel) return null;
  const exact = cards.find((c) => c.kind === sel.kind && c.key === sel.key);
  if (exact) return exact.id;
  if (sel.kind === 'net') return null;
  let best: Card | null = null;
  for (const c of cards) {
    if (c.kind === 'net') continue;
    if (c.key !== sel.key && under(sel.key, c.key) && (!best || c.key.length > best.key.length)) best = c;
  }
  return best?.id ?? null;
}

/** The selection a card stands for. */
export const selectionOfCard = (cards: Card[], id: string): Selection | null => {
  const c = cards.find((x) => x.id === id);
  return c ? { kind: c.kind, key: c.key } : null;
};
