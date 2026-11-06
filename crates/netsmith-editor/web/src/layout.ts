// The window's layout, as plain data and pure functions: which section each pane shows, and
// where the divider is. The page keeps it in the store and in localStorage.

/** The sections of the app. Exactly one fills a pane; a window has one pane or two. */
export type Section = 'schematic' | 'power' | 'system' | 'bom' | 'pcb' | 'build' | 'compare' | 'source';

/** The sections in the order of the tab strip. Their number keys are their places in it. */
export const SECTIONS: { id: Section; label: string }[] = [
  { id: 'source', label: 'Source' },
  { id: 'schematic', label: 'Schematic' },
  { id: 'power', label: 'Power tree' },
  { id: 'system', label: 'System' },
  { id: 'bom', label: 'BOM' },
  { id: 'pcb', label: 'PCB' },
  { id: 'build', label: 'Build' },
  { id: 'compare', label: 'Compare' },
];

const IDS = SECTIONS.map((s) => s.id);

export interface Layout {
  /** One section, or two (left, right); never the same one twice. */
  panes: Section[];
  /** The share of the width the first pane has, when there are two. */
  split: number;
  /** The pane that last had the pointer or the keys. */
  focus: number;
  /** Whether the person made the split (Split right, Ctrl+\, dragging a tab or the divider). A
   * split they did not make, one saved before this was kept, is not one a click-through opens
   * things in: it changes the pane that was clicked, as with one pane. */
  chosen: boolean;
}

/** A pane is never narrower than this many pixels. */
export const MIN_PANE = 380;
/** Below this window width, a saved split is not shown: one pane at a time. */
export const WIDE = 1400;

export const defaultLayout: Layout = { panes: ['schematic'], split: 0.5, focus: 0, chosen: false };

export const isSection = (x: unknown): x is Section => typeof x === 'string' && (IDS as string[]).includes(x);

/** A layout read from storage, made valid: anything it cannot trust becomes the default. */
export function parseLayout(raw: unknown): Layout {
  if (typeof raw !== 'object' || raw === null) return defaultLayout;
  const r = raw as Record<string, unknown>;
  const panes = Array.isArray(r.panes) ? r.panes.filter(isSection).slice(0, 2) : [];
  const distinct = panes.filter((p, i) => panes.indexOf(p) === i);
  if (distinct.length === 0) return defaultLayout;
  const split = typeof r.split === 'number' && Number.isFinite(r.split) ? Math.min(Math.max(r.split, 0.1), 0.9) : 0.5;
  const focus = typeof r.focus === 'number' && r.focus >= 1 && distinct.length === 2 ? 1 : 0;
  return { panes: distinct, split, focus, chosen: r.chosen === true && distinct.length === 2 };
}

/** Whether a window this wide shows two panes. */
export const canSplit = (width: number) => width >= WIDE;

/** What is on screen: for each visible pane, its section and its place in the saved layout.
 * A window too narrow for two panes shows the focused one and keeps the other saved. */
export function visiblePanes(l: Layout, width: number): { section: Section; index: number }[] {
  if (l.panes.length === 2 && !canSplit(width)) {
    const index = l.focus === 1 ? 1 : 0;
    return [{ section: l.panes[index], index }];
  }
  return l.panes.map((section, index) => ({ section, index }));
}

/** The first pane's share of `total` pixels, kept so that both panes are at least `min` wide.
 * A width too small for two panes gives half. */
export function clampSplit(share: number, total: number, min = MIN_PANE): number {
  if (!Number.isFinite(share)) return 0.5;
  if (total < 2 * min) return 0.5;
  const low = min / total;
  return Math.min(Math.max(share, low), 1 - low);
}

/** The section that makes a good partner for `section` in a second pane. */
export function companion(section: Section): Section {
  return section === 'source' ? 'schematic' : 'source';
}

/** Show `section` in pane `index`. When the other pane has it, the two swap, so no section is
 * ever in both. */
export function show(l: Layout, section: Section, index = l.focus): Layout {
  const at = Math.min(Math.max(index, 0), l.panes.length - 1);
  const panes = [...l.panes];
  const elsewhere = panes.indexOf(section);
  if (elsewhere >= 0 && elsewhere !== at) panes[elsewhere] = panes[at];
  panes[at] = section;
  return { ...l, panes, focus: at };
}

/** Split the window to put `section` on one side. With one pane, the old section stays on the
 * other side (a partner replaces it when it was `section` itself). With two, `section` goes into
 * the pane on that side. */
export function splitWith(l: Layout, section: Section, side: 'left' | 'right'): Layout {
  if (l.panes.length === 2) return { ...show(l, section, side === 'right' ? 1 : 0), chosen: true };
  const old = l.panes[0] === section ? companion(section) : l.panes[0];
  return side === 'right'
    ? { panes: [old, section], split: 0.5, focus: 1, chosen: true }
    : { panes: [section, old], split: 0.5, focus: 0, chosen: true };
}

/** The partner pane for a "Split right" with no section given: the partner of what is shown. */
export function splitRight(l: Layout): Layout {
  if (l.panes.length === 2) return l;
  return splitWith(l, companion(l.panes[0]), 'right');
}

/** Close pane `index`; the other one stays and fills the window. */
export function closePane(l: Layout, index: number): Layout {
  if (l.panes.length < 2) return l;
  const keep = l.panes[index === 0 ? 1 : 0];
  return { panes: [keep], split: 0.5, focus: 0, chosen: false };
}

/** Where `section` is on screen, if it is. */
export function whereShown(l: Layout, width: number, section: Section): number | null {
  const found = visiblePanes(l, width).find((p) => p.section === section);
  return found ? found.index : null;
}

/** The result of asking to show a section in the pane other than the one the click was in. */
export interface Opened {
  layout: Layout;
  /** What the pane showed before and where, so Back can put it back; null when nothing changed. */
  back: { pane: number; section: Section } | null;
  /** Whether it went into the pane the click was not in: a second pane the person may not expect. */
  other: boolean;
}

/** Click-through: show `section` (the source, say) for a click made in pane `from`. If it is on
 * screen already, only focus moves. Otherwise it replaces what the pane that was clicked shows,
 * and Back returns; only in a split the person made does it open in the other pane. */
export function openFrom(l: Layout, width: number, section: Section, from: number): Opened {
  const shown = whereShown(l, width, section);
  if (shown !== null) return { layout: { ...l, focus: shown }, back: null, other: false };
  const vis = visiblePanes(l, width);
  const clicked = (vis.find((p) => p.index === from) ?? vis[0]).index;
  const target = vis.length === 2 && l.chosen ? (vis.find((p) => p.index !== from) ?? vis[0]).index : clicked;
  return { layout: show(l, section, target), back: { pane: target, section: l.panes[target] }, other: vis.length === 2 && target !== clicked };
}

/** Put a section back where Back says. */
export function undoOpen(l: Layout, back: { pane: number; section: Section }): Layout {
  return show(l, back.section, Math.min(back.pane, l.panes.length - 1));
}

/** What a tab dropped on the window does. `side` is the half of a one-pane window it was
 * dropped on; `pane` is the pane of a two-pane window. */
export type Drop = { side: 'left' | 'right' } | { pane: number };

export function dropTab(l: Layout, section: Section, drop: Drop): Layout {
  if ('pane' in drop) return { ...show(l, section, drop.pane), chosen: true };
  return splitWith(l, section, drop.side);
}

/** Which drop a pointer makes, from where it is in the pane area (`x` and `left` and `width`
 * in pixels; `split` is the first pane's share). Null when the layout cannot take a drop. */
export function dropAt(l: Layout, width: number, x: number, left: number, areaWidth: number): Drop | null {
  if (!canSplit(width) || areaWidth <= 0) return null;
  const at = (x - left) / areaWidth;
  if (l.panes.length === 2) return { pane: at < l.split ? 0 : 1 };
  return { side: at < 0.5 ? 'left' : 'right' };
}

/** The section a number key picks (1 is the first), or null. */
export function sectionOfKey(key: string): Section | null {
  const n = Number(key);
  return Number.isInteger(n) && n >= 1 && n <= SECTIONS.length ? SECTIONS[n - 1].id : null;
}
