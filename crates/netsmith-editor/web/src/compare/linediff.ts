// A line diff, written for the comparison of a file at a git ref with the file on disk (ADR 0051):
// Myers' O(ND) shortest edit script over lines, with the common start and end taken off first so a
// file that changed in two places costs about the size of the changes. Pure: no page, no
// CodeMirror. The diff view, the gutter marks of the source and the status bar's count all read it.

/** One run of the edit script: lines `a[aFrom, aTo)` of the base became `b[bFrom, bTo)` of the head. */
export interface Hunk {
  kind: 'equal' | 'insert' | 'delete' | 'replace';
  aFrom: number;
  aTo: number;
  bFrom: number;
  bTo: number;
}

/** More differing lines than this and the middle is one replacement: Myers' trace grows with the
 * square of the differences, and nobody reads a diff of a rewritten file line by line. */
const MAX_EDITS = 2000;

/** The edit script turning `a` into `b`, as a list of runs in order that cover both arrays. */
export function diffLines(a: string[], b: string[]): Hunk[] {
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const runs: Hunk[] = [];
  if (start > 0) runs.push({ kind: 'equal', aFrom: 0, aTo: start, bFrom: 0, bTo: start });
  runs.push(...middle(a, b, start, endA, start, endB));
  if (endA < a.length) runs.push({ kind: 'equal', aFrom: endA, aTo: a.length, bFrom: endB, bTo: b.length });
  return merge(runs);
}

/** Adjacent runs of the same kind become one, and a delete next to an insert becomes a replacement. */
function merge(runs: Hunk[]): Hunk[] {
  const out: Hunk[] = [];
  for (const r of runs) {
    if (r.aFrom === r.aTo && r.bFrom === r.bTo) continue;
    const last = out[out.length - 1];
    if (last && last.kind === r.kind) {
      last.aTo = r.aTo;
      last.bTo = r.bTo;
      continue;
    }
    if (last && last.kind !== 'equal' && r.kind !== 'equal') {
      last.kind = 'replace';
      last.aTo = r.aTo;
      last.bTo = r.bTo;
      continue;
    }
    out.push({ ...r });
  }
  return out;
}

function middle(a: string[], b: string[], aLo: number, aHi: number, bLo: number, bHi: number): Hunk[] {
  const n = aHi - aLo;
  const m = bHi - bLo;
  if (n === 0 && m === 0) return [];
  if (n === 0) return [{ kind: 'insert', aFrom: aLo, aTo: aLo, bFrom: bLo, bTo: bHi }];
  if (m === 0) return [{ kind: 'delete', aFrom: aLo, aTo: aHi, bFrom: bLo, bTo: bLo }];
  const max = Math.min(n + m, MAX_EDITS);
  const offset = max + 1;
  const v = new Int32Array(2 * max + 3);
  // What `v` held when each round began, only the diagonals that round could read: -d-1 to d+1.
  const trace: Int32Array[] = [];
  let found = -1;
  for (let d = 0; d <= max && found < 0; d++) {
    trace.push(v.slice(offset - d - 1, offset + d + 2));
    for (let k = -d; k <= d; k += 2) {
      let x: number;
      if (k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1])) x = v[offset + k + 1];
      else x = v[offset + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[aLo + x] === b[bLo + y]) {
        x++;
        y++;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) {
        found = d;
        break;
      }
    }
  }
  if (found < 0) return [{ kind: 'replace', aFrom: aLo, aTo: aHi, bFrom: bLo, bTo: bHi }];
  // Walk the trace back from the end to the start, collecting the steps in reverse.
  const steps: ('=' | '+' | '-')[] = [];
  let x = n;
  let y = m;
  for (let d = found; d > 0; d--) {
    const vd = trace[d];
    const at = (diag: number) => vd[diag + d + 1];
    const k = x - y;
    const down = k === -d || (k !== d && at(k - 1) < at(k + 1));
    const prevK = down ? k + 1 : k - 1;
    const prevX = at(prevK);
    const prevY = prevX - prevK;
    while (x > prevX && y > prevY) {
      steps.push('=');
      x--;
      y--;
    }
    steps.push(down ? '+' : '-');
    x = prevX;
    y = prevY;
  }
  while (x > 0 && y > 0) {
    steps.push('=');
    x--;
    y--;
  }
  steps.reverse();
  const runs: Hunk[] = [];
  let ai = aLo;
  let bi = bLo;
  for (const s of steps) {
    if (s === '=') runs.push({ kind: 'equal', aFrom: ai, aTo: ++ai, bFrom: bi, bTo: ++bi });
    else if (s === '-') runs.push({ kind: 'delete', aFrom: ai, aTo: ++ai, bFrom: bi, bTo: bi });
    else runs.push({ kind: 'insert', aFrom: ai, aTo: ai, bFrom: bi, bTo: ++bi });
  }
  return runs;
}

/** The lines of a text, as an editor counts them: a text that ends in a newline has no empty last line. */
export function splitLines(text: string): string[] {
  if (text === '') return [];
  const lines = text.split('\n');
  if (lines[lines.length - 1] === '') lines.pop();
  return lines.map((l) => (l.endsWith('\r') ? l.slice(0, -1) : l));
}

/** What the margin of a buffer shows against its base: line numbers of the head (1-based). */
export interface GutterMarks {
  /** Lines that are new. */
  added: number[];
  /** Lines that replace others. */
  changed: number[];
  /** Head lines that follow lines that are gone (0 when they were the first ones). */
  deletedAfter: number[];
  /** Hunks that are not equal. */
  hunks: number;
  /** The lines that differ: a hunk counts the larger of its old and its new lines (a line edited is one line, not two). */
  lines: number;
}

export function gutterMarks(base: string[], head: string[]): GutterMarks {
  const marks: GutterMarks = { added: [], changed: [], deletedAfter: [], hunks: 0, lines: 0 };
  for (const h of diffLines(base, head)) {
    if (h.kind === 'equal') continue;
    marks.hunks++;
    marks.lines += Math.max(h.aTo - h.aFrom, h.bTo - h.bFrom);
    if (h.kind === 'delete') {
      marks.deletedAfter.push(h.bFrom);
      continue;
    }
    const lines = marks[h.kind === 'insert' ? 'added' : 'changed'];
    for (let i = h.bFrom; i < h.bTo; i++) lines.push(i + 1);
    // A replacement that has more new lines than old ones: the extra ones are new.
    if (h.kind === 'replace' && h.bTo - h.bFrom > h.aTo - h.aFrom) {
      const extra = h.bTo - h.bFrom - (h.aTo - h.aFrom);
      marks.changed.length -= extra;
      for (let i = h.bTo - extra; i < h.bTo; i++) marks.added.push(i + 1);
    }
    // ...and one that has fewer: lines are gone after it.
    if (h.kind === 'replace' && h.bTo - h.bFrom < h.aTo - h.aFrom) marks.deletedAfter.push(h.bTo);
  }
  return marks;
}

// ---- inside a line ----

/** A character range of a line, `[from, to)`. */
export type Span = [number, number];

/** Words, runs of spaces and single marks: what a person sees change in `.resistance &= 150ohm`. */
export function tokens(line: string): string[] {
  return line.match(/[A-Za-z0-9_]+|\s+|[^A-Za-z0-9_\s]/g) ?? [];
}

/** The parts of two lines that differ, as character spans of each. Tokens that are the same stay
 * plain; a line that is mostly different is not marked inside at all (the whole line says it). */
export function intraLine(a: string, b: string): { a: Span[]; b: Span[] } {
  const ta = tokens(a);
  const tb = tokens(b);
  const runs = diffLines(ta, tb);
  const spans = (side: 'a' | 'b', toks: string[]): Span[] => {
    const offsets: number[] = [0];
    for (const t of toks) offsets.push(offsets[offsets.length - 1] + t.length);
    const out: Span[] = [];
    for (const h of runs) {
      if (h.kind === 'equal') continue;
      const from = side === 'a' ? h.aFrom : h.bFrom;
      const to = side === 'a' ? h.aTo : h.bTo;
      if (from === to) continue;
      const s: Span = [offsets[from], offsets[to]];
      const last = out[out.length - 1];
      if (last && last[1] === s[0]) last[1] = s[1];
      else out.push(s);
    }
    return out;
  };
  const sa = spans('a', ta);
  const sb = spans('b', tb);
  const covered = (s: Span[], len: number) => s.reduce((n, [x, y]) => n + (y - x), 0) / Math.max(1, len);
  if (covered(sa, a.length) > 0.7 && covered(sb, b.length) > 0.7) return { a: [], b: [] };
  return { a: sa, b: sb };
}

// ---- rows for a diff view ----

export interface DiffRow {
  kind: 'equal' | 'add' | 'del';
  /** 1-based, absent for a line the side does not have. */
  aLine?: number;
  bLine?: number;
  text: string;
  /** Character spans that differ from the paired line. */
  marks: Span[];
}

/** The rows of an inline (unified) view: each base line that is gone, then each head line that is new. */
export function inlineRows(base: string[], head: string[]): DiffRow[] {
  const rows: DiffRow[] = [];
  for (const h of diffLines(base, head)) {
    if (h.kind === 'equal') {
      for (let i = 0; i < h.aTo - h.aFrom; i++) rows.push({ kind: 'equal', aLine: h.aFrom + i + 1, bLine: h.bFrom + i + 1, text: head[h.bFrom + i], marks: [] });
      continue;
    }
    const dels = h.aTo - h.aFrom;
    const adds = h.bTo - h.bFrom;
    const paired = Math.min(dels, adds);
    const delMarks: Span[][] = [];
    const addMarks: Span[][] = [];
    for (let i = 0; i < paired; i++) {
      const m = intraLine(base[h.aFrom + i], head[h.bFrom + i]);
      delMarks.push(m.a);
      addMarks.push(m.b);
    }
    for (let i = 0; i < dels; i++) rows.push({ kind: 'del', aLine: h.aFrom + i + 1, text: base[h.aFrom + i], marks: delMarks[i] ?? [] });
    for (let i = 0; i < adds; i++) rows.push({ kind: 'add', bLine: h.bFrom + i + 1, text: head[h.bFrom + i], marks: addMarks[i] ?? [] });
  }
  return rows;
}

/** The rows of a side-by-side view: left is the base, right the head, a gap where a side has no line. */
export interface SideRow {
  left: DiffRow | null;
  right: DiffRow | null;
}

export function sideRows(base: string[], head: string[]): SideRow[] {
  const rows: SideRow[] = [];
  for (const h of diffLines(base, head)) {
    if (h.kind === 'equal') {
      for (let i = 0; i < h.aTo - h.aFrom; i++) {
        const text = head[h.bFrom + i];
        rows.push({
          left: { kind: 'equal', aLine: h.aFrom + i + 1, text, marks: [] },
          right: { kind: 'equal', bLine: h.bFrom + i + 1, text, marks: [] },
        });
      }
      continue;
    }
    const dels = h.aTo - h.aFrom;
    const adds = h.bTo - h.bFrom;
    for (let i = 0; i < Math.max(dels, adds); i++) {
      const hasA = i < dels;
      const hasB = i < adds;
      const m = hasA && hasB ? intraLine(base[h.aFrom + i], head[h.bFrom + i]) : null;
      rows.push({
        left: hasA ? { kind: 'del', aLine: h.aFrom + i + 1, text: base[h.aFrom + i], marks: m?.a ?? [] } : null,
        right: hasB ? { kind: 'add', bLine: h.bFrom + i + 1, text: head[h.bFrom + i], marks: m?.b ?? [] } : null,
      });
    }
  }
  return rows;
}

/** Where each change starts, as an index into `rows` (for the buttons that go to the next one). */
export function changeStarts(rows: { kind: string }[] | { left: { kind: string } | null; right: { kind: string } | null }[]): number[] {
  const out: number[] = [];
  let inChange = false;
  rows.forEach((r, i) => {
    const kind = 'kind' in r ? r.kind : r.left?.kind === 'del' || r.right?.kind === 'add' ? 'change' : 'equal';
    const changed = kind !== 'equal';
    if (changed && !inChange) out.push(i);
    inChange = changed;
  });
  return out;
}
