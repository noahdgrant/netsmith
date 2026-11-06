// What the bill of materials view works out from the lines the server sends: designators as ranges,
// the sourcing status of a line, and the lines in groups. No page here, so it can be tested.

import type { BomLine } from './api';

/** `C1 C2 C3 C5 R1` as `C1–C3`, `C5`, `R1`: a run of three or more is a range, a pair is two names. */
export function designatorRanges(designators: string[]): string[] {
  const parsed = designators.map((d) => {
    const m = /^([A-Za-z_]+)(\d+)$/.exec(d);
    return m ? { d, prefix: m[1], n: Number(m[2]) } : { d, prefix: d, n: Number.NaN };
  });
  parsed.sort((a, b) => (a.prefix === b.prefix ? a.n - b.n : a.prefix.localeCompare(b.prefix)));
  const out: string[] = [];
  let i = 0;
  while (i < parsed.length) {
    let j = i;
    while (
      j + 1 < parsed.length &&
      !Number.isNaN(parsed[j].n) &&
      parsed[j + 1].prefix === parsed[i].prefix &&
      parsed[j + 1].n === parsed[j].n + 1
    ) {
      j++;
    }
    if (j - i >= 2) out.push(`${parsed[i].d}–${parsed[j].d}`);
    else for (let k = i; k <= j; k++) out.push(parsed[k].d);
    i = j + 1;
  }
  return out;
}

/** The designators of `all` that a chip (a name or a range) stands for. */
export function chipMembers(chip: string, all: string[]): string[] {
  const [a, b] = chip.split('\u2013');
  if (b === undefined) return [a];
  const pa = /^([A-Za-z_]+)(\d+)$/.exec(a);
  const pb = /^([A-Za-z_]+)(\d+)$/.exec(b);
  if (!pa || !pb) return [a];
  return all.filter((d) => {
    const m = /^([A-Za-z_]+)(\d+)$/.exec(d);
    return !!m && m[1] === pa[1] && Number(m[2]) >= Number(pa[2]) && Number(m[2]) <= Number(pb[2]);
  });
}

/** The first designator of a chip, which a click selects. */
export function firstOfRange(chip: string): string {
  return chip.split('–')[0];
}

export type Sourcing = 'found' | 'stale' | 'unchecked' | 'ambiguous' | 'missing' | 'hand';

/** What the page can say about where a line's part comes from. */
export function sourcing(l: BomLine): Sourcing {
  if (l.unresolved) return 'ambiguous';
  if (l.handSolder && !l.lcsc) return 'hand';
  if (!l.lcsc) return 'missing';
  if (l.date == null || l.unitUsd == null) return 'unchecked';
  return l.stale ? 'stale' : 'found';
}

export const SOURCING_LABEL: Record<Sourcing, string> = {
  found: 'Found',
  stale: 'Stale',
  unchecked: 'Not checked',
  ambiguous: 'Ambiguous',
  missing: 'Missing',
  hand: 'By hand',
};

/** A line that needs a person: no number, no single answer, or a price that is old. */
export const isProblem = (l: BomLine): boolean => ['ambiguous', 'missing', 'stale'].includes(sourcing(l));

export type GroupBy = 'none' | 'value' | 'module';

export interface Group {
  key: string;
  label: string;
  lines: BomLine[];
}

/** `stage.input_cap` is in the module `stage`; a part written at the top of the board is in no module. */
export const moduleOfPath = (path: string | undefined): string => (path?.includes('.') ? path.split('.')[0] : '');

/** The lines in groups. By module a line that spans two modules is split in two, each with its own designators. */
export function groupLines(lines: BomLine[], by: GroupBy, moduleOf: (designator: string) => string): Group[] {
  if (by === 'none') return [{ key: '', label: '', lines }];
  const groups = new Map<string, Group>();
  const put = (key: string, label: string, line: BomLine) => {
    let g = groups.get(key);
    if (!g) groups.set(key, (g = { key, label, lines: [] }));
    g.lines.push(line);
  };
  for (const l of lines) {
    if (by === 'value') {
      put(l.value, l.value, l);
      continue;
    }
    const split = new Map<string, string[]>();
    for (const d of l.designators) {
      const m = moduleOf(d);
      split.set(m, [...(split.get(m) ?? []), d]);
    }
    for (const [m, ds] of split) put(m, m || 'Top level of the board', split.size === 1 ? l : { ...l, designators: ds, quantity: ds.length });
  }
  return [...groups.values()].sort((a, b) => a.label.localeCompare(b.label, undefined, { numeric: true }));
}
