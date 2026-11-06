// The data a hover card and the inline hints ask for beyond the board's detail: the power tree (a
// net's rail), the bill of materials (a part's catalog line), the server's answer for a definition (its
// signature and doc comment), another board's detail (a module several boards use) and, for a server
// that does not answer with a signature, the text of a library file. Each is fetched once for the version of the analysis it
// belongs to, on the first card that wants it, and shared as a signal.

import { computed, signal } from '@preact/signals';

import type { BoardDetail, BomLine, Definition, PowerTree } from '../api';
import { api } from '../client';
import { analysis, board, openFiles } from '../store';

const key = computed(() => `${board.value ?? ''}|${analysis.value?.version ?? 0}`);

const powerBox = signal<{ key: string; tree: PowerTree | null } | null>(null);
const bomBox = signal<{ key: string; lines: BomLine[] } | null>(null);
const textBox = signal<Record<string, string>>({});
const flying = new Set<string>();

/** The power tree of the board shown, when it was fetched for this analysis. */
export const power = computed<PowerTree | null>(() => (powerBox.value?.key === key.value ? powerBox.value.tree : null));
/** The catalog lines of the board shown, when fetched for this analysis. */
export const bomLines = computed<BomLine[] | null>(() => (bomBox.value?.key === key.value ? bomBox.value.lines : null));

/** Ask for the power tree; a request already on its way is not repeated, and a failure is quiet. */
export function ensurePower(): void {
  const k = key.peek();
  const name = board.peek();
  if (!name || powerBox.peek()?.key === k || flying.has(`p|${k}`)) return;
  flying.add(`p|${k}`);
  api
    .power(name)
    .then((tree) => (powerBox.value = { key: k, tree }))
    .catch(() => (powerBox.value = { key: k, tree: null }))
    .finally(() => flying.delete(`p|${k}`));
}

export function ensureBom(): void {
  const k = key.peek();
  const name = board.peek();
  if (!name || bomBox.peek()?.key === k || flying.has(`b|${k}`)) return;
  flying.add(`b|${k}`);
  api
    .bom(name)
    .then((b) => (bomBox.value = { key: k, lines: [...b.lines, ...b.hand] }))
    .catch(() => (bomBox.value = { key: k, lines: [] }))
    .finally(() => flying.delete(`b|${k}`));
}

/** The line of the bill of materials a designator is in. */
export const bomLineOf = (lines: BomLine[] | null, refdes: string): BomLine | undefined => lines?.find((l) => l.designators.includes(refdes));

// What `/api/definition` answers for a name (its signature and its doc comment), asked for the text on the page, so a card for a
// definition needs no library file. `null` is an answer of "nothing" (or a failure); a key that is not there was not asked yet.
const defBox = signal<Record<string, Definition | null>>({});

const defKey = (path: string, offset: number, text: string) => `${analysis.peek()?.version ?? 0}|${path}|${offset}|${text.length}`;

/** The server's answer for the name at `offset` of `path`'s text: `undefined` while it is not in. */
export function servedDefinition(path: string, offset: number, text: string): Definition | null | undefined {
  return defBox.value[defKey(path, offset, text)];
}

/** Ask for the definition at `offset`, once for each version of the analysis and text. A failure is quiet (the card says what it has). */
export function ensureDefinition(path: string, offset: number, text: string): void {
  const k = defKey(path, offset, text);
  if (k in defBox.peek() || flying.has(`d|${k}`)) return;
  flying.add(`d|${k}`);
  api
    .definition(path, text, offset)
    .then((d) => (defBox.value = { ...defBox.value, [k]: d }))
    .catch(() => (defBox.value = { ...defBox.value, [k]: null }))
    .finally(() => flying.delete(`d|${k}`));
}

// The detail of a board other than the one shown, for a declaration of a module that several boards use: its card for each board.
const boardBox = signal<Record<string, BoardDetail | null>>({});

/** The detail of another board, when it was fetched for this analysis. */
export function detailOfBoard(name: string): BoardDetail | null | undefined {
  return boardBox.value[`${analysis.value?.version ?? 0}|${name}`];
}

export function ensureBoardDetail(name: string): void {
  const k = `${analysis.peek()?.version ?? 0}|${name}`;
  if (k in boardBox.peek() || flying.has(`board|${k}`)) return;
  flying.add(`board|${k}`);
  api
    .board(name)
    .then((d) => (boardBox.value = { ...boardBox.value, [k]: d }))
    .catch(() => (boardBox.value = { ...boardBox.value, [k]: null }))
    .finally(() => flying.delete(`board|${k}`));
}

/** The text of a file: the open one's (with edits), else what was fetched for a card. */
export function textOfFile(path: string): string | null {
  return openFiles.value[path]?.text ?? textBox.value[path] ?? null;
}

export function ensureText(path: string): void {
  if (openFiles.peek()[path] || path in textBox.peek() || flying.has(`t|${path}`)) return;
  flying.add(`t|${path}`);
  api
    .file(path)
    .then((f) => (textBox.value = { ...textBox.value, [path]: f.text }))
    .catch(() => (textBox.value = { ...textBox.value, [path]: '' }))
    .finally(() => flying.delete(`t|${path}`));
}
