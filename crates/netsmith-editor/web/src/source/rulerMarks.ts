// The marks of the source's overview ruler, the strip at the editor's right edge: one tick for each problem of the file and one
// for each run of lines the comparison with a ref changed, placed by their share of the file. Pure: lines and messages in,
// ticks out. The plugin (cm-ruler.ts) draws them and scrolls to a line when a tick is pressed.

export type ProblemKind = 'error' | 'warning';
export type ChangeKind = 'added' | 'changed' | 'deleted';

export interface RulerProblem {
  line: number;
  severity: ProblemKind;
  message: string;
}

export interface RulerChanges {
  added: Iterable<number>;
  changed: Iterable<number>;
  /** Lines that have lines gone above them. */
  deleted: Iterable<number>;
}

export interface Tick {
  /** Problems are in the lane on the right, changes in the one on the left. */
  lane: 'problem' | 'change';
  kind: ProblemKind | ChangeKind;
  /** 1-based, the first and the last line the tick stands for. */
  from: number;
  to: number;
  /** What the tooltip says: `line 12: message`. */
  label: string;
}

const CHANGE_WORDS: Record<ChangeKind, string> = {
  added: 'new since the ref',
  changed: 'changed since the ref',
  deleted: 'lines were removed here since the ref',
};

/** The first line of a message: a tooltip is one line, and a message can be several. */
const firstLine = (text: string): string => {
  const line = text.split('\n')[0].trim();
  return line.length > 140 ? `${line.slice(0, 139).trimEnd()}…` : line;
};

/** The ticks, in the order of their lines: each problem one tick, each run of consecutive lines of one kind of change one tick. */
export function rulerTicks(lines: number, problems: readonly RulerProblem[], changes: RulerChanges | null): Tick[] {
  const ticks: Tick[] = [];
  const clamp = (n: number) => Math.min(Math.max(1, n), Math.max(1, lines));
  for (const p of problems) {
    const line = clamp(p.line);
    ticks.push({ lane: 'problem', kind: p.severity, from: line, to: line, label: `line ${line}: ${firstLine(p.message)}` });
  }
  if (changes) {
    const kinds: [ChangeKind, Iterable<number>][] = [
      ['added', changes.added],
      ['changed', changes.changed],
      ['deleted', changes.deleted],
    ];
    for (const [kind, set] of kinds) {
      const sorted = [...new Set([...set].filter((n) => Number.isFinite(n)).map(clamp))].sort((a, b) => a - b);
      let from = 0;
      let to = 0;
      const flush = () => {
        if (!from) return;
        const where = from === to ? `line ${from}` : `lines ${from} to ${to}`;
        ticks.push({ lane: 'change', kind, from, to, label: `${where}: ${CHANGE_WORDS[kind]}` });
        from = 0;
      };
      for (const n of sorted) {
        if (from && n <= to + 1) to = n;
        else {
          flush();
          from = n;
          to = n;
        }
      }
      flush();
    }
  }
  return ticks.sort((a, b) => a.from - b.from || (a.lane === b.lane ? 0 : a.lane === 'problem' ? -1 : 1) || (a.kind === 'error' ? -1 : 0) - (b.kind === 'error' ? -1 : 0));
}

/** Where a tick or the thumb sits in a track of `track` pixels, for a document `whole` pixels high: a top and a height, at least `min` high. */
export function place(top: number, bottom: number, whole: number, track: number, min = 3): { top: number; height: number } {
  const scale = whole > 0 ? track / whole : 0;
  const t = Math.min(Math.max(0, top * scale), Math.max(0, track - min));
  return { top: t, height: Math.min(Math.max(min, (bottom - top) * scale), Math.max(min, track - t)) };
}

/** The document height a press at `y` of a track `track` pixels high stands for, for a document of `whole`. */
export const heightAt = (y: number, whole: number, track: number): number => (track > 0 ? Math.min(Math.max(0, y / track), 1) * whole : 0);
