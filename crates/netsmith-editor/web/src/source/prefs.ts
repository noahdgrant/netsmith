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
/** How comments and `///` docs are set in the source: upright in the code's font (the default), the old slanted one, or the page's
 * own reading font. Slanted monospace is the hardest of the three to read at a small size. */
export type CommentFont = 'upright' | 'italic' | 'sans';
export const COMMENT_FONTS: { id: CommentFont; label: string; hint: string; style: string; family: string }[] = [
  { id: 'upright', label: 'Code font', hint: 'Upright, in the font of the code: the default', style: 'normal', family: 'inherit' },
  { id: 'italic', label: 'Code font, slanted', hint: 'Slanted, in the font of the code', style: 'italic', family: 'inherit' },
  { id: 'sans', label: 'Reading font', hint: "Upright, in the page's own reading font", style: 'normal', family: 'var(--sans)' },
];
export const commentFont = kept<CommentFont>(
  'netsmith.source.commentFont.v1',
  'upright',
  (v): v is CommentFont => COMMENT_FONTS.some((f) => f.id === v),
);
export const commentFontOf = (id: CommentFont) => COMMENT_FONTS.find((f) => f.id === id) ?? COMMENT_FONTS[0];
export const nextCommentFont = (id: CommentFont): CommentFont => COMMENT_FONTS[(COMMENT_FONTS.findIndex((f) => f.id === id) + 1) % COMMENT_FONTS.length].id;
/** Vim keys in the source editor. */
export const vimMode = kept('netsmith.source.vim.v1', false);
/** Ctrl+S sorts the imports of an `.nsm` file first, in one undoable change. Autosave never does: it would move a line being typed. */
export const sortImportsOnSave = kept('netsmith.source.sortImportsOnSave.v1', false);
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

// The choice reaches the highlight styles (nsm.ts, languages.ts) through two custom properties on the page.
if (typeof document !== 'undefined') {
  effect(() => {
    const f = commentFontOf(commentFont.value);
    document.documentElement.style.setProperty('--comment-style', f.style);
    document.documentElement.style.setProperty('--comment-family', f.family);
  });
}
