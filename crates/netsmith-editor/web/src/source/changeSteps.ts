// Next change and Previous change in the source: the runs of lines the comparison with a ref marks in the open file (the same
// lines the margin's bars and the overview ruler show), stepped through from the cursor's line. Pure: lines in, a line out. The
// commands (commandset.ts) move the cursor to the line this gives.

export interface ChangedLines {
  added: Iterable<number>;
  changed: Iterable<number>;
  /** Lines that have lines gone above them. */
  deleted: Iterable<number>;
}

export interface Hunk {
  /** 1-based, the first and the last line of a run of consecutive marked lines (any kind of change). */
  from: number;
  to: number;
}

/** The runs of marked lines in the order of the file: lines of every kind next to each other are one change, as the diff counts it. */
export function changeHunks(lines: number, marks: ChangedLines): Hunk[] {
  const last = Math.max(1, lines);
  const all = new Set<number>();
  for (const set of [marks.added, marks.changed, marks.deleted]) for (const n of set) if (Number.isFinite(n)) all.add(Math.min(Math.max(1, Math.trunc(n)), last));
  const sorted = [...all].sort((a, b) => a - b);
  const out: Hunk[] = [];
  for (const n of sorted) {
    const tail = out[out.length - 1];
    if (tail && n <= tail.to + 1) tail.to = n;
    else out.push({ from: n, to: n });
  }
  return out;
}

/** The line a step goes to from `line`: the start of the next run (one that starts below the line), or of the one before (one that
 * ends above it). A cursor inside a run steps past that run, not to its own start. After the last run the next is the first, and the
 * previous before the first is the last, so the keys never end; `wrapped` says it did. Null when nothing is marked. */
export function stepChange(hunks: readonly Hunk[], line: number, dir: 1 | -1): { line: number; wrapped: boolean } | null {
  if (hunks.length === 0) return null;
  if (dir === 1) {
    const next = hunks.find((h) => h.from > line);
    return next ? { line: next.from, wrapped: false } : { line: hunks[0].from, wrapped: true };
  }
  for (let i = hunks.length - 1; i >= 0; i--) if (hunks[i].to < line) return { line: hunks[i].from, wrapped: false };
  return { line: hunks[hunks.length - 1].from, wrapped: true };
}
