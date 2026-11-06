// What the Compare section shows, worked out from the server's `Compare` (ADR 0051): the tiles, the
// rows of each tab, grouping and filtering, and the marks the other views carry. All of it is pure,
// so it is tested without a page. The words are the glossary's: a comparison of the circuit, never a diff.

import type {
  BomDiff,
  Compare,
  CompareBoard,
  CompareBomLine,
  CompareField,
  ComparePin,
  Location,
  Problem,
  Severity,
  SourceFile,
} from '../api';
import { ampsText } from '../power/format';
import { dollars, plural } from '../util';

export type Category = 'parts' | 'nets' | 'rails' | 'bom' | 'modules' | 'problems' | 'source';

export const CATEGORIES: { id: Category; label: string }[] = [
  { id: 'parts', label: 'Parts' },
  { id: 'nets', label: 'Nets' },
  { id: 'rails', label: 'Rails' },
  { id: 'bom', label: 'BOM' },
  { id: 'modules', label: 'Modules and interfaces' },
  { id: 'problems', label: 'Problems' },
  { id: 'source', label: 'Source' },
];

export type RowStatus = 'added' | 'removed' | 'changed' | 'renumbered' | 'renamed' | 'moved' | 'new' | 'resolved';

export const STATUS_LABEL: Record<RowStatus, string> = {
  added: 'Added',
  removed: 'Removed',
  changed: 'Changed',
  renumbered: 'Renumbered',
  renamed: 'Renamed',
  moved: 'Moved',
  new: 'New',
  resolved: 'Resolved',
};

export type RowKind = 'part' | 'net' | 'rail' | 'bom' | 'module' | 'port' | 'problem' | 'file';

/** A pin that joined or left a net, for the row's details. */
export interface PinChange {
  sign: '+' | '-';
  designator: string;
  pin: string;
  part: string;
  at?: Location;
}

export interface ChangeRow {
  id: string;
  category: Category;
  kind: RowKind;
  status: RowStatus;
  /** The board it is on (`TwoBoards/led`); empty for what belongs to none. */
  board: string;
  /** The path of a part or module, the name of a net or rail, the text of a BOM line or problem. */
  title: string;
  /** Its name at the ref, when it is not the same. */
  baseTitle?: string;
  /** What it is: `Resistor`, a module's definition, a severity. */
  type?: string;
  /** The old and the new of what changed, in two chips. */
  before?: string;
  after?: string;
  /** Which field the chips are of (`value`, `headroom`, `quantity`). */
  field?: string;
  /** Rails: whether the headroom got smaller or larger. */
  trend?: 'shrank' | 'grew';
  desBase?: string;
  desHead?: string;
  /** Every field that differs, for the expanded row. */
  fields: CompareField[];
  pins: PinChange[];
  /** The module a group by module puts it under: the path above a part, `''` for the board itself. */
  module: string;
  /** What a click selects in every view: absent for what is not on the head side. */
  select?: { kind: 'part' | 'net' | 'module'; key: string };
  base?: Location;
  head?: Location;
  severity?: Severity;
  /** Lowercase words a filter looks in. */
  search: string;
}

export const parentPath = (path: string): string => (path.includes('.') ? path.slice(0, path.lastIndexOf('.')) : '');

const fieldText = (v: string | undefined): string => (v === undefined || v === '' ? 'none' : v);

function row(r: Omit<ChangeRow, 'search' | 'fields' | 'pins' | 'module' | 'board'> & Partial<Pick<ChangeRow, 'fields' | 'pins' | 'module' | 'board'>>): ChangeRow {
  const full: ChangeRow = { fields: [], pins: [], module: '', board: '', ...r, search: '' };
  // The same name can be on two boards (`GND`, `3V3`): a row is the board's too, so a list of every board has unique keys.
  full.id = `${full.board}|${full.id}`;
  full.search = [full.title, full.baseTitle, full.type, full.before, full.after, full.desBase, full.desHead, full.status, STATUS_LABEL[full.status], ...full.fields.flatMap((f) => [f.field, f.old, f.new]), ...full.pins.map((p) => `${p.designator}.${p.pin} ${p.part}`)]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
  return full;
}

// ---- parts ----

/** The field a part's chips show: its value when that changed, else the first that did. */
export function headline(fields: CompareField[]): CompareField | undefined {
  return fields.find((f) => f.field === 'value') ?? fields.find((f) => f.field !== 'path' && f.field !== 'designator') ?? fields[0];
}

export function partRows(b: CompareBoard): ChangeRow[] {
  const out: ChangeRow[] = [];
  for (const p of b.parts.added) {
    out.push(row({ id: `part+${p.path}`, category: 'parts', kind: 'part', status: 'added', board: b.name, title: p.path, type: p.kind, after: p.value, field: 'value', desHead: p.designator, module: parentPath(p.path), select: { kind: 'part', key: p.path }, head: p.at, fields: partFields(p) }));
  }
  for (const p of b.parts.removed) {
    out.push(row({ id: `part-${p.path}`, category: 'parts', kind: 'part', status: 'removed', board: b.name, title: p.path, type: p.kind, before: p.value, field: 'value', desBase: p.designator, module: parentPath(p.path), base: p.at, fields: partFields(p) }));
  }
  for (const c of b.parts.changed) {
    const moved = c.fields.some((f) => f.field === 'path');
    const h = headline(c.fields);
    out.push(
      row({
        id: `part~${c.path}`,
        category: 'parts',
        kind: 'part',
        status: moved ? 'moved' : 'changed',
        board: b.name,
        title: c.path,
        baseTitle: c.basePath,
        before: h ? fieldText(h.old) : undefined,
        after: h ? fieldText(h.new) : undefined,
        field: h?.field,
        desBase: c.designatorBase,
        desHead: c.designatorHead,
        fields: c.fields,
        module: parentPath(c.path),
        select: { kind: 'part', key: c.path },
        head: c.at,
        base: c.baseAt,
      }),
    );
  }
  return out;
}

export function renumberedRows(b: CompareBoard): ChangeRow[] {
  return b.parts.renumbered.map((c) =>
    row({ id: `part#${c.path}`, category: 'parts', kind: 'part', status: 'renumbered', board: b.name, title: c.path, before: c.designatorBase, after: c.designatorHead, field: 'designator', desBase: c.designatorBase, desHead: c.designatorHead, fields: c.fields, module: parentPath(c.path), select: { kind: 'part', key: c.path }, head: c.at, base: c.baseAt }),
  );
}

function partFields(p: { kind: string; value: string; package?: string; lcsc?: string; mpn?: string; assembly: string }): CompareField[] {
  return [
    { field: 'kind', new: p.kind },
    { field: 'value', new: p.value },
    ...(p.package ? [{ field: 'package', new: p.package }] : []),
    ...(p.lcsc ? [{ field: 'lcsc', new: p.lcsc }] : []),
    ...(p.mpn ? [{ field: 'mpn', new: p.mpn }] : []),
    { field: 'assembly', new: p.assembly },
  ];
}

// ---- nets ----

const pinOf = (sign: '+' | '-', p: ComparePin): PinChange => ({ sign, designator: p.designator, pin: p.pin, part: p.part, at: p.at });

export function netRows(b: CompareBoard): ChangeRow[] {
  const out: ChangeRow[] = [];
  for (const n of b.nets.added) {
    out.push(row({ id: `net+${n.name}`, category: 'nets', kind: 'net', status: 'added', board: b.name, title: n.name, after: plural(n.pins.length, 'pin'), field: 'pins', pins: n.pins.map((p) => pinOf('+', p)), module: parentPath(n.pins[0]?.part ?? ''), select: { kind: 'net', key: n.name }, head: n.at }));
  }
  for (const n of b.nets.removed) {
    out.push(row({ id: `net-${n.name}`, category: 'nets', kind: 'net', status: 'removed', board: b.name, title: n.name, before: plural(n.pins.length, 'pin'), field: 'pins', pins: n.pins.map((p) => pinOf('-', p)), module: parentPath(n.pins[0]?.part ?? ''), base: n.at }));
  }
  for (const n of b.nets.renamed) {
    out.push(row({ id: `net>${n.name}`, category: 'nets', kind: 'net', status: 'renamed', board: b.name, title: n.name, baseTitle: n.baseName, before: n.baseName, after: n.name, field: 'name', pins: [...n.joined.map((p) => pinOf('+', p)), ...n.left.map((p) => pinOf('-', p))], module: parentPath((n.joined[0] ?? n.left[0])?.part ?? ''), select: { kind: 'net', key: n.name }, head: n.at, base: n.at }));
  }
  for (const n of b.nets.changed) {
    out.push(row({ id: `net~${n.name}`, category: 'nets', kind: 'net', status: 'changed', board: b.name, title: n.name, before: n.left.length ? `${n.left.length} left` : undefined, after: n.joined.length ? `${n.joined.length} joined` : undefined, field: 'pins', pins: [...n.joined.map((p) => pinOf('+', p)), ...n.left.map((p) => pinOf('-', p))], module: parentPath((n.joined[0] ?? n.left[0])?.part ?? ''), select: { kind: 'net', key: n.name }, head: n.at }));
  }
  return out;
}

// ---- rails ----

/** `80 mA`, or `unknown` for a rail that has no limit or no demand on that side. */
const headroomText = (a: number | undefined): string => (a === undefined ? 'unknown' : ampsText(a));

export function railRows(b: CompareBoard): ChangeRow[] {
  const out: ChangeRow[] = [];
  for (const r of b.rails.added) {
    out.push(row({ id: `rail+${r.net}`, category: 'rails', kind: 'rail', status: 'added', board: b.name, title: r.net, type: r.voltage?.text, after: r.voltage?.text, field: 'voltage', select: { kind: 'net', key: r.net }, head: r.at, fields: railFields(r) }));
  }
  for (const r of b.rails.removed) {
    out.push(row({ id: `rail-${r.net}`, category: 'rails', kind: 'rail', status: 'removed', board: b.name, title: r.net, type: r.voltage?.text, before: r.voltage?.text, field: 'voltage', base: r.at, fields: railFields(r) }));
  }
  for (const r of b.rails.changed) {
    const fields: CompareField[] = [];
    if (r.baseNet) fields.push({ field: 'name', old: r.baseNet, new: r.net });
    if (r.voltage) fields.push({ field: 'voltage', old: r.voltage.base?.text, new: r.voltage.head?.text });
    if (r.demand) fields.push({ field: 'demand', old: r.demand.base?.text, new: r.demand.head?.text });
    if (r.capacity) fields.push({ field: 'capacity', old: r.capacity.base?.text, new: r.capacity.head?.text });
    if (r.budget) fields.push({ field: 'budget', old: r.budget.base, new: r.budget.head });
    const t = headroomTrend(r.headroomBase, r.headroomHead);
    if (t) fields.unshift({ field: 'headroom', old: headroomText(r.headroomBase), new: headroomText(r.headroomHead) });
    const first = fields[0];
    out.push(
      row({
        id: `rail~${r.net}`,
        category: 'rails',
        kind: 'rail',
        status: 'changed',
        board: b.name,
        title: r.net,
        baseTitle: r.baseNet,
        before: first ? fieldText(first.old) : undefined,
        after: first ? fieldText(first.new) : undefined,
        field: first?.field,
        trend: t ?? undefined,
        fields,
        select: { kind: 'net', key: r.net },
        head: r.at,
        base: r.baseAt,
      }),
    );
  }
  return out;
}

function railFields(r: { voltage?: { text: string }; demand?: { text: string }; capacity?: { text: string }; budget: string }): CompareField[] {
  return [
    ...(r.voltage ? [{ field: 'voltage', new: r.voltage.text }] : []),
    ...(r.demand ? [{ field: 'demand', new: r.demand.text }] : []),
    ...(r.capacity ? [{ field: 'capacity', new: r.capacity.text }] : []),
    { field: 'budget', new: r.budget },
  ];
}

/** Whether the headroom (what the supply can give above the largest demand) got smaller or larger. */
export function headroomTrend(base: number | undefined, head: number | undefined): 'shrank' | 'grew' | null {
  if (base === undefined || head === undefined) return null;
  // To the nanoampere, as the server rounds it.
  const d = Math.round((head - base) * 1e9);
  return d < 0 ? 'shrank' : d > 0 ? 'grew' : null;
}

// ---- bill of materials ----

/** How `GET /api/bom` groups a line: what the server's bill of materials identifies it by (ADR 0051). */
export function bomKey(l: { kind: string; value: string; footprint?: string | null; lcsc?: string | null; mpn?: string | null; handSolder?: boolean | null }): string {
  return [l.kind, l.value, l.footprint ?? '', l.lcsc ?? '', l.mpn ?? '', l.handSolder ? 'hand' : ''].join('|');
}

const priceText = (v: number | undefined) => (v === undefined ? 'unknown' : dollars(v));

export function bomRows(b: CompareBoard): ChangeRow[] {
  const bom = b.bom;
  if (!bom) return [];
  const out: ChangeRow[] = [];
  const title = (l: { value: string; kind: string; package?: string }) => `${l.value} ${l.kind}`.trim();
  for (const l of bom.added) {
    out.push(row({ id: `bom+${bomKey(l)}`, category: 'bom', kind: 'bom', status: 'added', board: b.name, title: title(l), type: l.package ?? l.footprint, after: `qty ${l.quantity}`, field: 'quantity', desHead: l.designators.join(' '), fields: bomFields(l) }));
  }
  for (const l of bom.removed) {
    out.push(row({ id: `bom-${bomKey(l)}`, category: 'bom', kind: 'bom', status: 'removed', board: b.name, title: title(l), type: l.package ?? l.footprint, before: `qty ${l.quantity}`, field: 'quantity', desBase: l.designators.join(' '), fields: bomFields(l) }));
  }
  for (const c of bom.changed) {
    const fields: CompareField[] = [];
    if (c.quantityBase !== c.quantityHead) fields.push({ field: 'quantity', old: String(c.quantityBase), new: String(c.quantityHead) });
    if (c.unitUsdBase !== c.unitUsdHead) fields.push({ field: 'unit price', old: priceText(c.unitUsdBase), new: priceText(c.unitUsdHead) });
    const first = fields[0];
    out.push(row({ id: `bom~${bomKey(c)}`, category: 'bom', kind: 'bom', status: 'changed', board: b.name, title: title(c), type: c.package ?? c.footprint, before: first ? fieldText(first.old) : undefined, after: first ? fieldText(first.new) : undefined, field: first?.field, desBase: c.designatorsBase.join(' '), desHead: c.designatorsHead.join(' '), fields }));
  }
  return out;
}

function bomFields(l: CompareBomLine): CompareField[] {
  return [
    { field: 'quantity', new: String(l.quantity) },
    ...(l.package ? [{ field: 'package', new: l.package }] : []),
    ...(l.lcsc ? [{ field: 'lcsc', new: l.lcsc }] : []),
    ...(l.unitUsd !== undefined ? [{ field: 'unit price', new: dollars(l.unitUsd) }] : []),
  ];
}

// ---- modules and interfaces ----

export function moduleRows(b: CompareBoard): ChangeRow[] {
  const out: ChangeRow[] = [];
  for (const [list, kind] of [
    [b.modules, 'module'],
    [b.interfaces, 'port'],
  ] as const) {
    for (const i of list.added) out.push(row({ id: `${kind}+${i.path}`, category: 'modules', kind, status: 'added', board: b.name, title: i.path, type: i.typeName, after: i.typeName, field: 'type', module: parentPath(i.path), select: kind === 'module' ? { kind: 'module', key: i.path } : undefined, head: i.at }));
    for (const i of list.removed) out.push(row({ id: `${kind}-${i.path}`, category: 'modules', kind, status: 'removed', board: b.name, title: i.path, type: i.typeName, before: i.typeName, field: 'type', module: parentPath(i.path), base: i.at }));
    for (const i of list.changed) out.push(row({ id: `${kind}~${i.path}`, category: 'modules', kind, status: 'changed', board: b.name, title: i.path, type: i.headType, before: i.baseType, after: i.headType, field: 'type', fields: [{ field: 'type', old: i.baseType, new: i.headType }], module: parentPath(i.path), select: kind === 'module' ? { kind: 'module', key: i.path } : undefined, head: i.at }));
  }
  return out;
}

// ---- problems ----

export function problemRows(board: string, d: { new: Problem[]; resolved: Problem[] }): ChangeRow[] {
  const make = (p: Problem, status: 'new' | 'resolved', n: number): ChangeRow => {
    const at = p.at ?? undefined;
    return row({
      id: `problem${status}:${board}:${n}:${p.message}`,
      category: 'problems',
      kind: 'problem',
      status,
      board,
      title: p.message.split('\n')[0],
      type: p.stage,
      severity: p.severity,
      module: p.path ? parentPath(p.path) : '',
      select: p.path && status === 'new' ? { kind: 'part', key: p.path } : undefined,
      head: status === 'new' ? at : undefined,
      base: status === 'resolved' ? at : undefined,
    });
  };
  return [...d.new.map((p, i) => make(p, 'new', i)), ...d.resolved.map((p, i) => make(p, 'resolved', i))];
}

// ---- source ----

export const FILE_STATUS: Record<SourceFile['status'], string> = { modified: 'Modified', added: 'Added', deleted: 'Deleted', untracked: 'Untracked' };

// ---- the rows of one scope ----

/** Which boards a view of the comparison is about: one board by name, or all of them. */
export type Scope = string | 'all';

export const boardsIn = (c: Compare, scope: Scope): CompareBoard[] => (scope === 'all' ? c.boards : c.boards.filter((b) => b.name === scope));

export interface Rows {
  parts: ChangeRow[];
  renumbered: ChangeRow[];
  nets: ChangeRow[];
  rails: ChangeRow[];
  bom: ChangeRow[];
  modules: ChangeRow[];
  problems: ChangeRow[];
}

/** The rows of every tab for the boards in scope. The problems that name no board are in every scope. */
export function rowsOf(c: Compare, scope: Scope): Rows {
  const boards = boardsIn(c, scope);
  return {
    parts: boards.flatMap(partRows),
    renumbered: boards.flatMap(renumberedRows),
    nets: boards.flatMap(netRows),
    rails: boards.flatMap(railRows),
    bom: boards.flatMap(bomRows),
    modules: boards.flatMap(moduleRows),
    problems: [...problemRows('', c.problems), ...boards.flatMap((b) => problemRows(b.name, b.problems))],
  };
}

/** How many rows a tab has (the renumbered parts are counted apart, as the summary does). */
export function counts(c: Compare, scope: Scope): Record<Category, number> {
  const r = rowsOf(c, scope);
  return { parts: r.parts.length, nets: r.nets.length, rails: r.rails.length, bom: r.bom.length, modules: r.modules.length, problems: r.problems.length, source: c.source.files.length };
}

/** The circuit changes of the boards in scope: parts, nets, rails, modules and ports, not the bill of materials, the problems or
 * the source's lines. This one number is the tiles' total in the section, and the chip's in the status bar when the scope is all. */
export function circuitChanges(c: Compare | null, scope: Scope = 'all'): number {
  if (!c || c.error) return 0;
  return boardsIn(c, scope).reduce((n, b) => n + boardChanges(b), 0);
}

/** What the status chip counts: the circuit changes of every board. */
export const changeCount = (c: Compare | null): number => circuitChanges(c, 'all');

/** The sentence under the title for the scope shown: the server's own for all the boards, else the board's own, so the
 * words and the tiles are about the same boards. */
export function scopeSummary(c: Compare, scope: Scope): string {
  if (scope === 'all') return c.summary;
  const b = c.boards.find((x) => x.name === scope);
  return b ? b.summary.trim() || 'No circuit changes' : c.summary;
}

/** The lines of the source that differ across the whole comparison, for the chip's tooltip. */
export const sourceLines = (c: Compare): number => c.source.added + c.source.removed;

// ---- grouping and filtering ----

export type GroupBy = 'none' | 'module' | 'change';

export interface RowGroup {
  key: string;
  label: string;
  rows: ChangeRow[];
}

export function filterRows(rows: ChangeRow[], query: string): ChangeRow[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return words.length === 0 ? rows : rows.filter((r) => words.every((w) => r.search.includes(w)));
}

const ORDER: RowStatus[] = ['added', 'removed', 'changed', 'moved', 'renamed', 'renumbered', 'new', 'resolved'];

/** Rows in groups: none (one list, as the server sorted them), by module path (the board's own first), or by kind of change. */
export function groupRows(rows: ChangeRow[], by: GroupBy): RowGroup[] {
  if (by === 'none') return rows.length ? [{ key: '', label: '', rows }] : [];
  const map = new Map<string, ChangeRow[]>();
  for (const r of rows) {
    const key = by === 'module' ? r.module : r.status;
    map.set(key, [...(map.get(key) ?? []), r]);
  }
  const keys = [...map.keys()].sort((a, b) =>
    by === 'change' ? ORDER.indexOf(a as RowStatus) - ORDER.indexOf(b as RowStatus) : a === '' ? -1 : b === '' ? 1 : a.localeCompare(b, undefined, { numeric: true }),
  );
  return keys.map((key) => ({ key, label: by === 'change' ? STATUS_LABEL[key as RowStatus] : key === '' ? 'On the board' : key, rows: map.get(key)! }));
}

// ---- tiles ----

export interface TileChip {
  sign: '+' | '-' | '~';
  n: number;
  label: string;
}

export interface Tile {
  id: 'parts' | 'nets' | 'rails' | 'bom' | 'problems';
  label: string;
  value: string;
  sub?: string;
  tone: 'neutral' | 'ok' | 'warn' | 'bad';
  chips: TileChip[];
  /** The tab a click on the tile opens. */
  tab: Category;
  /** Not a figure (`unknown`): drawn quiet, with what to say about it in `tip`. */
  muted?: boolean;
  tip?: string;
}

const chips = (a: number, r: number, c: number, changedLabel = 'changed'): TileChip[] => [
  ...(a ? [{ sign: '+' as const, n: a, label: 'added' }] : []),
  ...(r ? [{ sign: '-' as const, n: r, label: 'removed' }] : []),
  ...(c ? [{ sign: '~' as const, n: c, label: changedLabel }] : []),
];

/** The delta of cost as a person writes it: `+$0.012`, `-$0.30`, `no change`, `unknown`. */
export function costDelta(d: number | undefined | null): string {
  if (d === undefined || d === null) return 'unknown';
  if (Math.abs(d) < 5e-7) return 'no change';
  return `${d > 0 ? '+' : '-'}${dollars(Math.abs(d))}`;
}

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

/** The stat tiles for the boards in scope. */
export function tiles(c: Compare, scope: Scope): Tile[] {
  const boards = boardsIn(c, scope);
  const n = <T,>(f: (b: CompareBoard) => T[]) => sum(boards.map((b) => f(b).length));
  const partsA = n((b) => b.parts.added);
  const partsR = n((b) => b.parts.removed);
  const partsC = n((b) => b.parts.changed);
  const renum = n((b) => b.parts.renumbered);
  const netsA = n((b) => b.nets.added);
  const netsR = n((b) => b.nets.removed);
  const netsC = n((b) => b.nets.renamed) + n((b) => b.nets.changed);
  const railsA = n((b) => b.rails.added);
  const railsR = n((b) => b.rails.removed);
  const railsC = n((b) => b.rails.changed);
  const shrank = sum(boards.map((b) => b.rails.changed.filter((r) => headroomTrend(r.headroomBase, r.headroomHead) === 'shrank').length));
  const bomA = n((b) => b.bom?.added ?? []);
  const bomR = n((b) => b.bom?.removed ?? []);
  const bomC = n((b) => b.bom?.changed ?? []);
  const deltas = boards.map((b) => b.bom?.deltaUsd);
  const delta = deltas.length > 0 && deltas.every((d) => d !== undefined && d !== null) ? sum(deltas as number[]) : undefined;
  const bomChanged = bomA + bomR + bomC > 0;
  const bomCostUnknown = boards.some((b) => b.bom === undefined) && !bomChanged ? true : bomChanged && delta === undefined;
  const probs = [c.problems, ...boards.map((b) => b.problems)];
  const pNew = sum(probs.map((p) => p.new.length));
  const pRes = sum(probs.map((p) => p.resolved.length));
  return [
    { id: 'parts', label: 'Parts', value: String(partsA + partsR + partsC), sub: renum ? `${renum} renumbered` : undefined, tone: 'neutral', chips: chips(partsA, partsR, partsC), tab: 'parts' },
    { id: 'nets', label: 'Nets', value: String(netsA + netsR + netsC), tone: 'neutral', chips: chips(netsA, netsR, netsC), tab: 'nets' },
    { id: 'rails', label: 'Rails', value: String(railsA + railsR + railsC), sub: shrank ? `${plural(shrank, 'headroom')} shrank` : undefined, tone: shrank ? 'warn' : 'neutral', chips: chips(railsA, railsR, railsC), tab: 'rails' },
    {
      id: 'bom',
      label: 'BOM cost',
      value: boards.some((b) => b.bom === undefined) && !bomChanged ? 'unknown' : bomChanged ? costDelta(delta) : 'no change',
      sub: bomChanged ? `${plural(bomA + bomR + bomC, 'line')} differ` : 'no line differs',
      tone: delta === undefined || delta === 0 ? 'neutral' : delta > 0 ? 'warn' : 'ok',
      chips: chips(bomA, bomR, bomC),
      tab: 'bom',
      ...(bomCostUnknown ? { muted: true, tip: 'No prices in the catalogs yet' } : {}),
    },
    { id: 'problems', label: 'Problems', value: pNew === 0 && pRes === 0 ? 'None' : `${pNew} new`, sub: pRes ? `${pRes} resolved` : undefined, tone: pNew > 0 ? 'bad' : pRes > 0 ? 'ok' : 'neutral', chips: [...(pNew ? [{ sign: '+' as const, n: pNew, label: 'new' }] : []), ...(pRes ? [{ sign: '-' as const, n: pRes, label: 'resolved' }] : [])], tab: 'problems' },
  ];
}

/** Whether only comments or whitespace changed: the source differs and the circuit does not. */
export const onlySourceChanged = (c: Compare): boolean => !c.changed && !c.error && c.source.filesChanged > 0;

/** How many changes to the circuit a board has: parts, nets, rails, modules and ports. */
export function boardChanges(b: CompareBoard): number {
  const inst = (d: { added: unknown[]; removed: unknown[]; changed: unknown[] }) => d.added.length + d.removed.length + d.changed.length;
  return (
    b.parts.added.length + b.parts.removed.length + b.parts.changed.length +
    b.nets.added.length + b.nets.removed.length + b.nets.renamed.length + b.nets.changed.length +
    b.rails.added.length + b.rails.removed.length + b.rails.changed.length +
    inst(b.modules) + inst(b.interfaces)
  );
}

/** What a list of boards says of each: `3 changes`, `new board`, `removed`, `unchanged`. */
export function boardStatusText(b: CompareBoard): string {
  switch (b.status) {
    case 'added':
      return 'new board';
    case 'removed':
      return 'removed';
    case 'unchanged':
      return 'unchanged';
    default: {
      const n = boardChanges(b);
      return n === 0 ? 'changed' : plural(n, 'change');
    }
  }
}

// ---- the stage the server is in ----

export interface StageInfo {
  stage?: string;
  message: string;
  done?: number;
  total?: number;
}

const STAGES: Record<string, string> = {
  extract: 'Reading the files at the ref from git',
  check: 'Checking the sources at the ref',
  solve: 'Solving the values at the ref',
  lower: 'Lowering the boards at the ref',
  draw: 'Drawing the schematics at the ref',
  modules: 'Reading the modules at the ref',
  diff: 'Comparing the two readings',
};

/** The stage text of a comparison in progress: the server's own message when it has one, else the stage in words; never a percentage. */
export function stageText(p: StageInfo | null): string {
  if (!p) return 'Starting';
  const base = p.message.trim() || (p.stage ? (STAGES[p.stage] ?? p.stage) : 'Working');
  return base.charAt(0).toUpperCase() + base.slice(1);
}

/** The stages in the order the server goes through them, for the row of dots under the text. */
export const STAGE_ORDER = ['extract', 'check', 'solve', 'lower', 'draw', 'modules', 'diff'] as const;

export const stageIndex = (stage: string | undefined): number => (stage ? STAGE_ORDER.indexOf(stage as (typeof STAGE_ORDER)[number]) : -1);

// ---- the marks the other views carry ----

export type PartMark = 'added' | 'changed' | 'renumbered';

export interface RailMark {
  mark: 'added' | 'changed';
  /** `80 mA -> 20 mA`, when the headroom changed. */
  delta?: string;
  trend?: 'shrank' | 'grew';
}

export interface Marks {
  /** By the path of the part on the head side. */
  parts: Map<string, PartMark>;
  /** By the name of the net on the head side. */
  nets: Map<string, 'added' | 'changed'>;
  /** By the path of a module: it holds a part that differs, or it is itself new or changed. */
  modules: Map<string, 'added' | 'changed'>;
  rails: Map<string, RailMark>;
  /** Parts that are gone: the head side has nothing to draw, so the inspector lists them. */
  removedParts: { path: string; designator: string; kind: string; value: string; module: string; at?: Location }[];
  removedNets: string[];
  status: CompareBoard['status'];
}

/** The marks for one board of a comparison; none for a board that is not in it. */
export function marksOf(b: CompareBoard | undefined): Marks {
  const m: Marks = { parts: new Map(), nets: new Map(), modules: new Map(), rails: new Map(), removedParts: [], removedNets: [], status: b?.status ?? 'unchanged' };
  if (!b) return m;
  const touch = (path: string) => {
    for (let p = parentPath(path); p !== ''; p = parentPath(p)) if (!m.modules.has(p)) m.modules.set(p, 'changed');
  };
  for (const p of b.parts.added) {
    m.parts.set(p.path, 'added');
    touch(p.path);
  }
  for (const c of b.parts.changed) {
    m.parts.set(c.path, 'changed');
    touch(c.path);
  }
  for (const c of b.parts.renumbered) m.parts.set(c.path, 'renumbered');
  for (const p of b.parts.removed) {
    m.removedParts.push({ path: p.path, designator: p.designator, kind: p.kind, value: p.value, module: parentPath(p.path), at: p.at });
    touch(p.path);
  }
  for (const n of b.nets.added) m.nets.set(n.name, 'added');
  for (const n of [...b.nets.renamed, ...b.nets.changed]) m.nets.set(n.name, 'changed');
  m.removedNets = b.nets.removed.map((n) => n.name);
  for (const i of [...b.modules.added]) m.modules.set(i.path, 'added');
  for (const i of [...b.modules.changed]) m.modules.set(i.path, 'changed');
  for (const r of b.rails.added) m.rails.set(r.net, { mark: 'added' });
  for (const r of b.rails.changed) {
    const trend = headroomTrend(r.headroomBase, r.headroomHead) ?? undefined;
    m.rails.set(r.net, { mark: 'changed', trend, delta: trend ? `${headroomText(r.headroomBase)} → ${headroomText(r.headroomHead)}` : undefined });
  }
  return m;
}

/** What the page's drawings and lists get: a CSS selector and the class (and a delta text) to put on what it finds. The
 * subscriber in `compareDom.ts` applies these to the page; this is only the assignment, which is tested. */
export interface MarkTarget {
  selector: string;
  cls: string;
  /** For a rail: the text of the badge, shown from the `data-cmp-delta` attribute by CSS. */
  delta?: string;
}

const q = (v: string) => `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;

/** The designator of a part by its path, as the head's board says (the schematic is drawn by designator). */
export type DesignatorOf = (path: string) => string | undefined;

/** A rail card of the power tree for `net`: the card of that board when the page says whose it is (a system's cards carry
 * `data-cmp-board`), else, for the board in front, any card of that net. */
const railSelector = (net: string, board: string | undefined, unqualified: boolean): string[] => {
  const base = `.pt-rail[data-hv=${q(`net:${net}`)}]`;
  return [...(board ? [`${base}[data-cmp-board=${q(board)}]`] : []), ...(unqualified ? [`${base}:not([data-cmp-board])`] : [])];
};

function railMark(net: string, r: RailMark, board: string | undefined, unqualified: boolean): MarkTarget[] {
  const cls = `cmp-${r.mark}${r.trend ? ` cmp-${r.trend}` : ''}`;
  return railSelector(net, board, unqualified).map((selector) => ({ selector, cls, delta: r.delta }));
}

/** The rails of a board that is not the one in front: only the power tree draws them, as the cards of that board. */
export function otherBoardTargets(m: Marks, board: string): MarkTarget[] {
  return [...m.rails].flatMap(([net, r]) => railMark(net, r, board, false));
}

/** `board`: the short name of the board in front (`led` of `TwoBoards/led`), when the comparison has several. */
export function markTargets(m: Marks, designatorOf: DesignatorOf, board?: string): MarkTarget[] {
  const out: MarkTarget[] = [];
  for (const [path, mark] of m.parts) {
    const cls = `cmp-${mark}`;
    const ref = designatorOf(path);
    if (ref) out.push({ selector: `[data-ref=${q(ref)}]:not(.pin)`, cls });
    out.push({ selector: `[data-hv=${q(`part:${path}`)}]`, cls }, { selector: `[data-path=${q(path)}]`, cls });
  }
  for (const [path, mark] of m.modules) out.push({ selector: `[data-hv=${q(`module:${path}`)}]`, cls: `cmp-${mark}` });
  for (const [net, mark] of m.nets) {
    // A wire is softly lit; a rail card or a chip of the net is marked too.
    out.push({ selector: `[data-net=${q(net)}]:not(.pin)`, cls: `cmp-net-${mark}` }, { selector: `[data-hv=${q(`net:${net}`)}]`, cls: `cmp-net-${mark}` });
  }
  for (const [net, r] of m.rails) out.push(...railMark(net, r, board, true));
  return out;
}

/** The marks of a system's boards, by the board's own name (`led` of `TwoBoards/led`). */
export function boardMarks(c: Compare | null): Map<string, CompareBoard['status']> {
  const out = new Map<string, CompareBoard['status']>();
  for (const b of c?.boards ?? []) if (b.status !== 'unchanged') out.set(b.name.includes('/') ? b.name.slice(b.name.lastIndexOf('/') + 1) : b.name, b.status);
  return out;
}

// ---- the bill of materials ----

export interface BomMark {
  status: 'added' | 'changed';
  quantityBase?: number;
  quantityHead: number;
  /** Dollars per board: `unitUsdHead - unitUsdBase` times the new quantity, when both prices are known. */
  deltaUsd?: number;
  unitBase?: number;
  unitHead?: number;
}

export interface BomMarks {
  byKey: Map<string, BomMark>;
  removed: CompareBomLine[];
}

export function bomMarks(d: BomDiff | undefined): BomMarks {
  const byKey = new Map<string, BomMark>();
  for (const l of d?.added ?? []) byKey.set(bomKey(l), { status: 'added', quantityHead: l.quantity, unitHead: l.unitUsd, deltaUsd: l.unitUsd === undefined ? undefined : l.unitUsd * l.quantity });
  for (const c of d?.changed ?? []) {
    const delta = c.unitUsdBase !== undefined && c.unitUsdHead !== undefined ? c.unitUsdHead * c.quantityHead - c.unitUsdBase * c.quantityBase : undefined;
    byKey.set(bomKey(c), { status: 'changed', quantityBase: c.quantityBase, quantityHead: c.quantityHead, unitBase: c.unitUsdBase, unitHead: c.unitUsdHead, deltaUsd: delta });
  }
  return { byKey, removed: d?.removed ?? [] };
}

/** `+2`, `-1` for a change of quantity. */
export const signed = (n: number): string => (n > 0 ? `+${n}` : String(n));

// ---- removed parts, on the sheet they were on ----

/** The sheet of the schematic a removed part was on: the sheet of the parts that still share its module, else of those
 * below it, else of the nearest module above that has any. The head has no drawing of it, so this is a guess from its
 * neighbours; undefined when its module is gone altogether. */
export function sheetOfRemoved(module: string, parts: { path: string; refdes: string }[], partSheet: ReadonlyMap<string, string>): string | undefined {
  for (let m = module; ; m = parentPath(m)) {
    const here = parts.find((p) => parentPath(p.path) === m);
    const below = parts.find((p) => m !== '' && p.path.startsWith(`${m}.`));
    const hit = here ?? below;
    if (hit) return partSheet.get(hit.refdes);
    if (m === '') return undefined;
  }
}

/** The removed parts the sheet `current` lists: all of them in the overview (`all`), else those that were on that sheet. */
export function removedOn(removed: Marks['removedParts'], parts: { path: string; refdes: string }[], partSheet: ReadonlyMap<string, string>, current: string, all: string): Marks['removedParts'] {
  return current === all ? removed : removed.filter((r) => sheetOfRemoved(r.module, parts, partSheet) === current);
}
