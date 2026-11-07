// What the person chose for the source, kept in localStorage: whether hover cards and inline hints
// show, whether long lines wrap, the size of the text, and the outline panel's state. Each is a
// signal; the editor and the commands read and write them.

import { effect, signal } from '@preact/signals';

import { load, save } from '../storage';
import type { FoldChoice } from './outline';

function kept<T>(key: string, fallback: T, valid: (v: unknown) => v is T = (v): v is T => typeof v === typeof fallback) {
  const stored = load<unknown>(key, fallback);
  const s = signal<T>(valid(stored) ? stored : fallback);
  effect(() => save(key, s.value));
  return s;
}

export const FONT = { min: 10, max: 22, initial: 13 };
export const clampFont = (n: number): number => Math.min(FONT.max, Math.max(FONT.min, Math.round(n)));

/** The card that shows after a short rest on a part, a net or a definition. */
export const hoverCards = kept('netsmith.source.hoverCards.v1', true);
/** The muted value and part number at the end of a declaration's line. */
export const inlineHints = kept('netsmith.source.inlineHints.v1', true);
export const wordWrap = kept('netsmith.source.wrap.v1', false);
/** A file is saved a moment after the last edit, with no Ctrl+S. Off until chosen: a save builds the project again. */
export const autosave = kept('netsmith.source.autosave.v1', false);
export const fontSize = kept('netsmith.source.fontSize.v1', FONT.initial, (v): v is number => typeof v === 'number' && v >= FONT.min && v <= FONT.max);
/** The outline panel at the foot of the file tree: open or folded, and how tall. */
export const outlineOpen = kept('netsmith.source.outline.v1', true);
export const OUTLINE_H = { min: 120, max: 520, initial: 260 };
export const outlineHeight = kept('netsmith.source.outlineHeight.v1', OUTLINE_H.initial, (v): v is number => typeof v === 'number' && v >= OUTLINE_H.min && v <= OUTLINE_H.max);

/** What the person opened and folded in the outline of each file (by the names of the entries), the last forty files. */
export const outlineFolds = kept<Record<string, FoldChoice>>('netsmith.source.outlineFolds.v1', {}, (v): v is Record<string, FoldChoice> => !!v && typeof v === 'object' && !Array.isArray(v));
export function setFoldChoice(path: string, choice: FoldChoice) {
  const next: Record<string, FoldChoice> = { ...outlineFolds.peek() };
  // Most recent last: the oldest go when there are more than forty.
  delete next[path];
  next[path] = choice;
  for (const k of Object.keys(next).slice(0, Math.max(0, Object.keys(next).length - 40))) delete next[k];
  outlineFolds.value = next;
}

export const zoomEditor = (by: number) => (fontSize.value = clampFont(fontSize.value + by));
export const resetEditorZoom = () => (fontSize.value = FONT.initial);

// The text size reaches the editor and the outline through one custom property on the page: the editor's own size (Ctrl+= and Ctrl+-)
// times the page's Size setting (`--ui-scale`, 0.9 to 1.25), so 14 px at 125 % is 17.5 px, and the two are independent.
if (typeof document !== 'undefined') {
  effect(() => {
    document.documentElement.style.setProperty('--source-fs', `calc(${fontSize.value}px * var(--ui-scale, 1))`);
  });
}
