// What the server says of a comparison while it runs: `progress` messages with `task: "compare"`,
// and `board` the ref as asked. A leaf module, so the page's socket handler can write it without
// importing the comparison's store.

import { signal } from '@preact/signals';

export interface CompareProgress {
  /** The ref as asked. */
  ref: string;
  stage?: string;
  done: number;
  total: number;
  message: string;
  /** When this comparison began, for the elapsed time. */
  since: number;
}

export const compareProgress = signal<CompareProgress | null>(null);

/** The page asked for a comparison of `ref`: the clock starts. */
export function beginCompareProgress(ref: string) {
  compareProgress.value = { ref, done: 0, total: 0, message: '', since: Date.now() };
}

export function endCompareProgress() {
  compareProgress.value = null;
}

/** A `progress` message of the task `compare`. Said about another ref than the one asked for now, it is ignored. */
export function noteCompareProgress(m: { board: string; message: string; stage?: string; done?: number; total?: number }) {
  const p = compareProgress.peek();
  if (!p || p.ref !== m.board) return;
  compareProgress.value = { ...p, stage: m.stage ?? p.stage, message: m.message, done: m.done ?? 0, total: m.total ?? 0 };
}
