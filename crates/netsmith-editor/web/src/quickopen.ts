// What go to anything searches, and how: the index of a board's parts, nets and modules and of
// the project's files and definitions, the prefix modes of the input (`>` commands, `@` modules
// and definitions, `#` nets, `:` a line), and the ranking. Pure functions of the data the editor
// already sends (`BoardDetail`, `Project`); no page, no store.

import type { BoardDetail, Project, Severity } from './api';
import { ORIGIN_LABEL, type Row, whereText } from './feedback/problems';
import { type Field, matchFields } from './fuzzy';
import { type Selection, modulesOf, moduleSel, netSel, partSel } from './selection';

export type ResultKind = 'command' | 'problem' | 'part' | 'net' | 'bus' | 'module' | 'definition' | 'file';

/** What choosing a result does. */
export type Target =
  | { type: 'selection'; sel: Selection }
  /** A bus the schematic draws, by the path of its interface instance. */
  | { type: 'bus'; path: string }
  | { type: 'definition'; file: string; line: number; name: string; defKind: string }
  | { type: 'file'; path: string }
  | { type: 'command'; id: string }
  | { type: 'problem'; id: string };

export interface Item {
  /** Unique, stable: kept to bring recent choices back. */
  id: string;
  kind: ResultKind;
  /** The name in bold. */
  label: string;
  /** The words after the name in the same line (the value of a part). */
  sub?: string;
  /** The line under or beside it: the path, the file and line. */
  detail: string;
  /** What is matched; each field is shown (`label`, `sub`, `detail`) or hidden. */
  fields: Field[];
  /** The key chord of a command. */
  keys?: string;
  /** The severity of a problem, for its icon. */
  tone?: Severity;
  target: Target;
}

export interface Index {
  parts: Item[];
  nets: Item[];
  buses: Item[];
  modules: Item[];
  definitions: Item[];
  files: Item[];
}

/** A bus of the drawing, as go to anything needs it: the instance, its definition and its members. */
export interface BusEntry {
  path: string;
  type: string;
  nets: string[];
  members: { name: string; net: string }[];
}

const basename = (p: string) => p.split('/').pop() ?? p;
const dirname = (p: string) => (p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : '');

export function buildIndex(detail: BoardDetail | null, project: Project | null, drawn: BusEntry[] = []): Index {
  const parts: Item[] = (detail?.parts ?? []).map((p) => {
    // The line under the name is made of the pieces that can be searched, each marked where it falls.
    const pieces = [p.path, p.kind, p.package, p.lcsc].filter((x): x is string => !!x);
    const at: number[] = [];
    pieces.reduce((from, x) => (at.push(from), from + x.length + 3), 0);
    const shift = (text: string | null) => (text ? at[pieces.indexOf(text)] : 0);
    return {
      id: `part:${p.path}`,
      kind: 'part' as const,
      label: p.refdes,
      sub: p.value,
      detail: pieces.join(' · '),
      fields: [
        { name: 'label', text: p.refdes, weight: 3 },
        { name: 'sub', text: p.value, weight: 1.3 },
        { name: 'path', text: p.path, weight: 1, into: 'detail', shift: shift(p.path) },
        { name: 'kind', text: p.kind, weight: 0.8, into: 'detail', shift: shift(p.kind) },
        { name: 'package', text: p.package ?? '', weight: 0.4, into: 'detail', shift: shift(p.package) },
        { name: 'lcsc', text: p.lcsc ?? '', weight: 0.5, into: 'detail', shift: shift(p.lcsc) },
      ],
      target: { type: 'selection' as const, sel: partSel(p.path) },
    };
  });
  const nets: Item[] = (detail?.nets ?? []).map((n) => ({
    id: `net:${n.name}`,
    kind: 'net',
    label: n.name,
    detail: `net · ${n.pins.length} pin${n.pins.length === 1 ? '' : 's'}${n.named ? '' : ' · unnamed'}`,
    fields: [{ name: 'label', text: n.name, weight: 2 }],
    target: { type: 'selection', sel: netSel(n.name) },
  }));
  // A bus is the interface instance the drawing draws as one line; it is named by the instance's own
  // name, and found by the instance's path, its definition or one of its members' names.
  const buses: Item[] = drawn.map((b) => {
    const name = b.path.slice(b.path.lastIndexOf('.') + 1);
    return {
      id: `bus:${b.path}`,
      kind: 'bus' as const,
      label: name,
      sub: b.type,
      detail: `bus · ${b.nets.length} nets · ${b.path}`,
      fields: [
        { name: 'label', text: name, weight: 2.4 },
        { name: 'sub', text: b.type, weight: 1 },
        { name: 'path', text: b.path, weight: 0.8, into: 'detail', shift: `bus · ${b.nets.length} nets · `.length },
      ],
      target: { type: 'bus' as const, path: b.path },
    };
  });
  const modules: Item[] = modulesOf(detail).map((m) => ({
    id: `module:${m.path}`,
    kind: 'module',
    label: m.path,
    detail: `module · ${m.parts.length} part${m.parts.length === 1 ? '' : 's'}${m.decl ? ` · ${m.decl.file}:${m.decl.line}` : ''}`,
    fields: [{ name: 'label', text: m.path, weight: 2 }],
    target: { type: 'selection', sel: moduleSel(m.path) },
  }));
  const definitions: Item[] = [];
  const files: Item[] = [];
  for (const f of project?.files ?? []) {
    files.push({
      id: `file:${f.path}`,
      kind: 'file',
      label: basename(f.path),
      detail: dirname(f.path) || 'project root',
      fields: [
        { name: 'label', text: basename(f.path), weight: 2 },
        { name: 'detail', text: f.path, weight: 1 },
      ],
      target: { type: 'file', path: f.path },
    });
    // The standard library's definitions are listed after the project's, and rank below in a tie.
    for (const d of f.defs) {
      definitions.push({
        id: `def:${f.path}:${d.line}:${d.name}`,
        kind: 'definition',
        label: d.name,
        sub: d.kind,
        detail: `${f.path}:${d.line}`,
        fields: [
          { name: 'label', text: d.name, weight: 2 },
          { name: 'sub', text: d.kind, weight: 0.6 },
        ],
        target: { type: 'definition', file: f.path, line: d.line, name: d.name, defKind: d.kind },
      });
    }
  }
  return { parts, nets, buses, modules, definitions, files };
}

export type Mode = 'all' | 'command' | 'problem' | 'symbol' | 'net' | 'line';

export const PREFIXES: { prefix: string; mode: Mode; title: string; hint: string }[] = [
  { prefix: '>', mode: 'command', title: 'Commands', hint: 'run a command' },
  { prefix: '@', mode: 'symbol', title: 'Modules and definitions', hint: 'a module of the board or a definition' },
  { prefix: '#', mode: 'net', title: 'Nets and buses', hint: 'a net of the board, or a bus' },
  { prefix: ':', mode: 'line', title: 'Line', hint: 'a line of the open file' },
  { prefix: '!', mode: 'problem', title: 'Problems', hint: 'an error, warning or note of the project' },
];

/** The rows of the problems dock as items, in the order given (the dock's order). */
export function problemItems(rows: Row[]): Item[] {
  return rows.map((r) => ({
    id: `problem:${r.id}`,
    kind: 'problem' as const,
    label: r.message,
    detail: [ORIGIN_LABEL[r.origin], whereText(r) || r.path || r.board || ''].filter(Boolean).join(' · '),
    tone: r.severity,
    fields: [
      { name: 'label', text: r.message, weight: 2 },
      { name: 'detail', text: r.full, weight: 0.6 },
    ],
    target: { type: 'problem' as const, id: r.id },
  }));
}

/** The mode a query is in, and the words after the prefix. */
export function parseQuery(raw: string): { mode: Mode; text: string } {
  const p = PREFIXES.find((x) => raw.startsWith(x.prefix));
  return p ? { mode: p.mode, text: raw.slice(1).trim() } : { mode: 'all', text: raw.trim() };
}

export interface Hit {
  item: Item;
  score: number;
  /** The marked positions of the shown fields (`label`, `sub`, `detail`). */
  marks: Record<string, number[]>;
}

export interface Group {
  title: string;
  hits: Hit[];
}

/** How many of a kind the mixed list shows. */
const CAP: Record<string, number> = { part: 12, net: 8, bus: 4, module: 6, definition: 8, file: 8 };
const CAP_ALONE = 80;
/** A match scoring under this share of the best in its group is dropped. */
const WEAK = 0.3;

function rank(items: Item[], text: string, cap: number): Hit[] {
  const hits: Hit[] = [];
  for (const item of items) {
    const m = matchFields(text, item.fields);
    if (m) hits.push({ item, score: m.score, marks: m.marks });
  }
  // The sort is stable: equal scores stay in the order of the list (designators in order).
  hits.sort((a, b) => b.score - a.score);
  // What scores far below the best match is noise (a part whose LCSC number starts like the
  // designator typed): left out, but the best match always stays.
  const floor = hits.length > 0 ? hits[0].score * WEAK : 0;
  return hits.filter((h, i) => i === 0 || h.score >= floor).slice(0, cap);
}

const first = (n: number, items: Item[]) => items.slice(0, n).map((item): Hit => ({ item, score: 0, marks: {} }));

/** The groups for a query, best group first. `commands` are the items for the command mode. */
export function search(index: Index, commands: Item[], raw: string, problems: Item[] = []): Group[] {
  const { mode, text } = parseQuery(raw);
  const groups: Group[] = [];
  const add = (title: string, hits: Hit[]) => hits.length > 0 && groups.push({ title, hits });
  switch (mode) {
    case 'command':
      return [{ title: '', hits: text ? rank(commands, text, CAP_ALONE) : first(CAP_ALONE, commands) }].filter((g) => g.hits.length);
    case 'problem':
      return [{ title: '', hits: text ? rank(problems, text, CAP_ALONE) : first(CAP_ALONE, problems) }].filter((g) => g.hits.length);
    case 'net':
      // The buses of the drawing come first: a net of the board is one of their members, or not.
      add('Buses', text ? rank(index.buses, text, CAP_ALONE) : first(CAP_ALONE, index.buses));
      add('Nets', text ? rank(index.nets, text, CAP_ALONE) : first(CAP_ALONE, index.nets));
      return groups;
    case 'symbol':
      add('Modules', text ? rank(index.modules, text, CAP_ALONE) : first(CAP_ALONE, index.modules));
      add('Definitions', text ? rank(index.definitions, text, CAP_ALONE) : first(CAP_ALONE, index.definitions));
      return groups;
    case 'line':
      return [];
    case 'all': {
      if (!text) return [];
      const list: [string, Item[]][] = [
        ['Parts', index.parts],
        ['Nets', index.nets],
        ['Buses', index.buses],
        ['Modules', index.modules],
        ['Definitions', index.definitions],
        ['Files', index.files],
      ];
      for (const [title, items] of list) add(title, rank(items, text, CAP[items[0]?.kind ?? 'part'] ?? 8));
      // The group with the best match first, so the thing most likely meant is on top.
      return groups.sort((a, b) => b.hits[0].score - a.hits[0].score);
    }
  }
}

/** `:42` → the line asked for, and whether the open file has it. */
export function parseLine(text: string, lines: number | null): { line: number | null; message: string } {
  const t = text.trim();
  if (t === '') return { line: null, message: lines ? `Type a line number between 1 and ${lines}.` : 'Open a file to go to a line in it.' };
  const n = Number(t);
  if (!Number.isInteger(n) || n < 1) return { line: null, message: `“${t}” is not a line number.` };
  if (lines != null && n > lines) return { line: lines, message: `The file has ${lines} lines: go to the last one.` };
  return { line: n, message: `Go to line ${n}.` };
}

/** The items of the recent choices that are still in the index, most recent first. */
export function recentItems(index: Index, ids: string[]): Item[] {
  const all = new Map<string, Item>();
  for (const list of [index.parts, index.nets, index.buses, index.modules, index.definitions, index.files]) for (const i of list) all.set(i.id, i);
  return ids.map((id) => all.get(id)).filter((i): i is Item => !!i);
}
