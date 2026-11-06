// The data a hover card and the inline hints ask for beyond the board's detail: the power tree (a
// net's rail), the bill of materials (a part's catalog line) and the text of a library file (a
// definition's signature and doc comment). Each is fetched once for the version of the analysis it
// belongs to, on the first card that wants it, and shared as a signal.

import { computed, signal } from '@preact/signals';

import type { BomLine, PowerTree } from '../api';
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
