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

/** The notes of a sheet that are lit while parts are: the note whose first part (`data-ref`) is one of `refs`. */
export function litNotes(notes: { ref: string | null }[], refs: ReadonlySet<string>): number[] {
  const out: number[] = [];
  notes.forEach((n, i) => {
    if (n.ref && refs.has(n.ref)) out.push(i);
  });
  return out;
}
