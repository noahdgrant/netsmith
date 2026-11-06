// Which runs of unchanged lines a diff view hides, and what it remembers about them. Pure: rows go in, index ranges come out.
// The view (compare/cm-diff.ts) draws each run as a slim bar, `N unchanged lines`, that opens on a click or Enter; the
// panel (views/compare/DiffPanel.tsx) keeps the choice for the page's session.

import { signal } from '@preact/signals';

/** Lines of context kept on each side of a change. */
export const CONTEXT = 3;
/** A run is hidden only when it hides at least this many lines (a bar for fewer would not be shorter than the lines). */
export const MIN_HIDDEN = 3;
/** A file longer than this hides its unchanged lines until the person says not to. */
export const AUTO_OVER = 60;

/** Rows `[from, to)` of a view that are hidden behind one bar. */
export interface Region {
  from: number;
  to: number;
}

/** The runs of unchanged rows to hide, given for each row whether it changed: all but `context` rows next to a change. A view with
 * nothing changed hides nothing (there is nothing to look at but the file itself). */
export function collapsedRegions(changed: readonly boolean[], context = CONTEXT, minHidden = MIN_HIDDEN): Region[] {
  const n = changed.length;
  if (!changed.some(Boolean)) return [];
  const out: Region[] = [];
  let i = 0;
  while (i < n) {
    if (changed[i]) {
      i++;
      continue;
    }
    let j = i;
    while (j < n && !changed[j]) j++;
    // Rows next to a change stay; the start and the end of the file have a change on one side only.
    const from = i === 0 ? 0 : i + context;
    const to = j === n ? n : j - context;
    if (to - from >= minHidden) out.push({ from, to });
    i = j;
  }
  return out;
}

/** Whether a file of `lines` rows hides its unchanged lines: what the person chose, else on for a long file. */
export const hidesUnchanged = (choice: boolean | null, lines: number): boolean => choice ?? lines > AUTO_OVER;

/** The region that holds row `row`, if any. */
export const regionAt = <T extends Region>(regions: readonly T[], row: number): T | undefined => regions.find((r) => row >= r.from && row < r.to);

/** What the bar says: `12 unchanged lines`. */
export const barText = (n: number): string => `${n} unchanged ${n === 1 ? 'line' : 'lines'}`;

// ---- what the page remembers while it is open ----

/** The person's choice of the toolbar's switch (null: not chosen, so the file's length decides). */
export const hideChoice = signal<boolean | null>(null);

const opened = new Map<string, Set<number>>();
const keyOf = (file: string) => file;

/** The bars opened in a file, each by the base line number its hidden run starts at (the same in both ways of showing the changes). */
export function openedBars(file: string): ReadonlySet<number> {
  return opened.get(keyOf(file)) ?? new Set();
}

export function openBar(file: string, line: number) {
  const set = new Set(opened.get(keyOf(file)));
  set.add(line);
  opened.set(keyOf(file), set);
}

/** Close every bar of a file (the switch turned on again, or a new comparison). */
export function resetBars(file?: string) {
  if (file === undefined) opened.clear();
  else opened.delete(keyOf(file));
}

/** What the open comparison offers the commands (F7, Shift+F7, the palette): set by the panel while it is on screen. */
export interface DiffControls {
  step: (direction: 1 | -1) => void;
  toggleHide: () => void;
  /** Whether the unchanged lines are hidden now. */
  hiding: boolean;
  /** How many changes there are (no stepping without any). */
  changes: number;
}

export const diffControls = signal<DiffControls | null>(null);
