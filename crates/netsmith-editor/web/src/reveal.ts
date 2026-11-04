// Where the cursor goes when a click-through reveals a place in the source (a definition in the
// tree, a part in the schematic, a problem, a power-tree row), and the short flash that shows
// the line it went to.
//
// The place is never a one-letter selection: CodeMirror marks every other occurrence of the
// selected text, so a selection of `b` lit every `b` in the file. A place the server gives as
// a span of two or more characters on one line is selected; anything shorter or longer is a
// cursor at its start, and the flash shows where it is.

import { StateEffect, StateField, type Extension, type Range } from '@codemirror/state';
import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';

export interface Place {
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

/** What `reveal` needs of a document: CodeMirror's `Text` has it. */
export interface Doc {
  lines: number;
  line(n: number): { from: number; to: number; text: string };
}

export interface Target {
  /** The selection: the same position twice is a cursor with nothing selected. */
  anchor: number;
  head: number;
  /** The line to scroll to and flash (1-based). */
  line: number;
}

/** The start of the name `name` on a line, after the keyword that comes first (`board Beta`). */
export function nameColumn(text: string, name: string): number | null {
  const first = /^\s*(\S+)/.exec(text);
  let from = first ? first.index + first[0].length : 0;
  for (;;) {
    const at = text.indexOf(name, from);
    if (at < 0) return null;
    const word = /[\p{L}\p{N}_]/u;
    const before = at > 0 ? text[at - 1] : '';
    const after = text[at + name.length] ?? '';
    if (!word.test(before) && !word.test(after)) return at;
    from = at + 1;
  }
}

/** Where to put the cursor for a place. `name`, when given, is the definition whose line it is:
 * the tree knows a line only, so the cursor goes to the name on it. */
export function revealTarget(doc: Doc, at: Place, name?: string): Target {
  const clamp = (n: number) => Math.min(Math.max(n, 1), doc.lines);
  const first = doc.line(clamp(at.line));
  const last = doc.line(clamp(at.endLine));
  if (name) {
    const column = nameColumn(first.text, name);
    const lead = first.text.length - first.text.trimStart().length;
    const pos = first.from + (column ?? lead);
    return { anchor: pos, head: pos, line: clamp(at.line) };
  }
  const from = Math.min(first.from + at.column - 1, first.to);
  const to = Math.min(last.from + at.endColumn - 1, last.to);
  const whole = clamp(at.endLine) === clamp(at.line) && to - from >= 2;
  return whole ? { anchor: from, head: to, line: clamp(at.line) } : { anchor: from, head: from, line: clamp(at.line) };
}

/** Marks a line for a moment. The value says which of two identical animations to use, so
 * the same line asked for twice in a row flashes twice. */
export const flashLine = StateEffect.define<{ pos: number; swap: boolean }>();

const flashField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(set, tr) {
    set = set.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(flashLine)) continue;
      const line = tr.state.doc.lineAt(e.value.pos);
      const mark: Range<Decoration> = Decoration.line({ class: e.value.swap ? 'cm-reveal-b' : 'cm-reveal-a' }).range(line.from);
      set = Decoration.set([mark]);
    }
    return set;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const flashTheme = EditorView.baseTheme({
  '@keyframes cm-reveal-a': { from: { backgroundColor: 'rgba(232, 135, 58, .38)' }, to: { backgroundColor: 'transparent' } },
  '@keyframes cm-reveal-b': { from: { backgroundColor: 'rgba(232, 135, 58, .38)' }, to: { backgroundColor: 'transparent' } },
  '.cm-reveal-a': { animation: 'cm-reveal-a 1.4s ease-out' },
  '.cm-reveal-b': { animation: 'cm-reveal-b 1.4s ease-out' },
  '@media (prefers-reduced-motion: reduce)': { '.cm-reveal-a, .cm-reveal-b': { animation: 'none' } },
});

export const revealFlash: Extension = [flashField, flashTheme];

/** The next value for `flashLine`: alternates, so the animation starts over. */
let swap = false;
export function nextFlash(pos: number) {
  swap = !swap;
  return { pos, swap };
}
