// The one hover of the page: what the pointer (or the keyboard focus) is over, as a part, a net, a
// module or a bus. Like the selection it is shared: a row of the bill of materials, a chip of an
// inspector, a card of the power tree, a part drawn in the schematic or on the board each say what
// they stand for, and every view that shows the same thing lights it softly (the token
// `--hover-lit`, never the colour of a pick). It is transient: it is set when the pointer settles
// (a short delay, so a pass over a list does not flicker the drawings) and cleared when it leaves,
// when focus goes, on Escape, and never set by a touch.
//
// This file is the store and the pure resolving. `hoverDom.ts` is the one subscriber that toggles
// a class on what is lit, so that no view re-renders for a hover.

import { signal } from '@preact/signals';

import type { BoardDetail } from './api';
import { partByPath, refdesOf, under } from './selection';

export type HoverKind = 'part' | 'net' | 'module' | 'bus';

export interface Hover {
  kind: HoverKind;
  /** The instance path of a part or a module, the name of a net, the instance path of a bus. */
  key: string;
  /** Designators that stand for the thing besides what its key gives (a sheet of the navigator, a definition's parts). */
  refs?: string[];
  /** Nets that stand for it (a bus's members). */
  nets?: string[];
}

/** The thing the pointer is over, or null. */
export const hover = signal<Hover | null>(null);

export const sameHover = (a: Hover | null, b: Hover | null): boolean => a === b || (!!a && !!b && a.kind === b.kind && a.key === b.key);

/** How long the pointer rests on a thing before it lights, and how long after it left before it goes
 * out (moving from one row to the next keeps the light moving, not blinking). */
export const HOVER_DELAY_MS = 70;
export const HOVER_LEAVE_MS = 90;

let enterTimer: ReturnType<typeof setTimeout> | undefined;
let leaveTimer: ReturnType<typeof setTimeout> | undefined;

/** Set the hover now. */
export function setHover(h: Hover | null) {
  clearTimeout(enterTimer);
  clearTimeout(leaveTimer);
  if (!sameHover(hover.peek(), h)) hover.value = h;
}

/** The pointer or focus came to `h`: it lights after a short rest, at once when something is lit already. */
export function hoverEnter(h: Hover) {
  clearTimeout(leaveTimer);
  clearTimeout(enterTimer);
  if (sameHover(hover.peek(), h)) {
    // The same thing, maybe with other sets: take them.
    if (hover.peek() !== h) hover.value = h;
    return;
  }
  if (hover.peek()) hover.value = h;
  else enterTimer = setTimeout(() => (hover.value = h), HOVER_DELAY_MS);
}

/** The pointer or focus left (what it was on, when given): the light goes out after a moment unless it came to another thing. */
export function hoverLeave(h?: Hover) {
  clearTimeout(enterTimer);
  if (h && !sameHover(hover.peek(), h)) return;
  clearTimeout(leaveTimer);
  leaveTimer = setTimeout(() => (hover.value = null), HOVER_LEAVE_MS);
}

/** Put the light out at once (Escape, a view change). */
export function clearHover() {
  setHover(null);
}

/** The handlers of an element that stands for `h`: pointer and focus both say it, a touch does not. The element also
 * carries `data-hv`, which is how the views that list things are found when something else is hovered. */
export function hoverAttrs(h: Hover | null | (() => Hover | null)) {
  const get = () => (typeof h === 'function' ? h() : h);
  const now = get();
  return {
    'data-hv': now ? `${now.kind}:${now.key}` : undefined,
    onPointerEnter: (e: PointerEvent) => {
      const v = get();
      if (v && e.pointerType !== 'touch') hoverEnter(v);
    },
    onPointerLeave: (e: PointerEvent) => {
      const v = get();
      if (v && e.pointerType !== 'touch') hoverLeave(v);
    },
    onFocus: () => {
      const v = get();
      if (v) hoverEnter(v);
    },
    onBlur: () => {
      const v = get();
      if (v) hoverLeave(v);
    },
  };
}

/** The hover of the parts with these designators (a row of the bill of materials, a pin chip): the first one's path is its key. */
export function hoverOfRefs(d: BoardDetail | null, refs: string[]): Hover | null {
  const p = d?.parts.find((x) => x.refdes === refs[0]);
  return p ? { kind: 'part', key: p.path, refs } : null;
}

/** What a hover stands for in the board: the designators, the instance paths of parts, the nets, and the module keys. */
export interface Lit {
  /** The designators a list lights (a row of the bill of materials): the parts on a hovered net too. */
  refs: Set<string>;
  /** The designators a drawing lights: a hovered net lights its wires and tracks, not every part on it. */
  drawn: Set<string>;
  paths: Set<string>;
  nets: Set<string>;
  /** The module (or part) path the hover is at, for a card that stands for the module around it. */
  at: string | null;
}

export function resolveHover(d: BoardDetail | null, h: Hover | null): Lit {
  const lit: Lit = { refs: new Set(h?.refs ?? []), drawn: new Set(h?.refs ?? []), paths: new Set(), nets: new Set(h?.nets ?? []), at: null };
  if (!h) return lit;
  if (d) {
    for (const r of lit.refs) {
      const p = d.parts.find((x) => x.refdes === r);
      if (p) lit.paths.add(p.path);
    }
  }
  if (h.kind === 'net') lit.nets.add(h.key);
  if (h.kind === 'part' || h.kind === 'module') lit.at = h.key;
  if (d) {
    if (h.kind === 'part') {
      const p = partByPath(d, h.key);
      if (p) {
        lit.refs.add(p.refdes);
        lit.drawn.add(p.refdes);
        lit.paths.add(p.path);
      }
    } else if (h.kind === 'net') {
      for (const r of refdesOf(d, { kind: 'net', key: h.key })) lit.refs.add(r);
    } else if (h.kind === 'module') {
      for (const p of d.parts) {
        if (!under(p.path, h.key)) continue;
        lit.refs.add(p.refdes);
        lit.drawn.add(p.refdes);
        lit.paths.add(p.path);
      }
    }
  }
  return lit;
}

/** The parts a definition of the source has: those declared in its file from its line to the next definition's. */
export function partsOfDefinition(d: BoardDetail | null, file: string, line: number, nextLine: number | null): string[] {
  if (!d) return [];
  return d.parts.filter((p) => p.decl?.file === file && p.decl.line >= line && (nextLine === null || p.decl.line < nextLine)).map((p) => p.refdes);
}

/** Of the module keys in `keys`, those a hover at `at` lights: the nearest module around it (a part of a converter
 * lights the converter) and any module below it (a module lights the cards of what it holds). */
export function modulesLit(keys: string[], at: string | null): Set<string> {
  const out = new Set<string>();
  if (!at) return out;
  let best: string | null = null;
  for (const k of keys) {
    if (under(at, k) && (best === null || k.length > best.length)) best = k;
    else if (under(k, at)) out.add(k);
  }
  if (best) out.add(best);
  return out;
}

/** The selector of everything that stands for `lit` in the page: the drawings (`data-ref` of a part, `data-net`) and what lists. */
export function selectorOf(lit: Lit, modules: Set<string>): string {
  const q = (v: string) => `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
  const parts: string[] = [];
  for (const r of lit.drawn) parts.push(`[data-ref=${q(r)}]:not(.pin)`);
  for (const r of lit.refs) parts.push(`[data-hv-ref~=${q(r)}]`);
  for (const p of lit.paths) parts.push(`[data-hv=${q(`part:${p}`)}]`);
  for (const n of lit.nets) parts.push(`[data-net=${q(n)}]:not(.pin)`, `[data-hv=${q(`net:${n}`)}]`);
  for (const m of modules) parts.push(`[data-hv=${q(`module:${m}`)}]`);
  return parts.join(',');
}
