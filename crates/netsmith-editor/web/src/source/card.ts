// What a hover card in the source says. Two steps, both pure: `targetAt` reads the token under the
// pointer (a declaration's name, a type, a string that names a net) from the line and the file's
// symbols; `cardFor` builds the card from what the page knows of the board (`BoardDetail`, the power
// tree, the project's files) and, for a definition, the text of its file. Nothing is fetched here:
// a card that needs a file's text or the power tree is built without it and again when it is there.

import type { Analysis, Budget, Def, BoardDetail, Definition, Location, ModuleUse, PartInfo, PowerTree, Problem, Project, Range, Solved } from '../api';
import { tokenize } from '../nsm';
import { type Selection, type ModuleInfo, modulesOf, netSel, partSel, moduleSel } from '../selection';
import { spaceUnits } from '../util';
import { type Role, type Sym, containing, docAbove, flatten, roleOf, scanSymbols } from './symbols';

// ---- what is under the pointer ----

export type Target =
  /** A declaration (`.r1: Resistor`), found by its name or by the root of a path (`.r1.resistance`). */
  | { type: 'decl'; sym: Sym; owner: Sym | null }
  /** A type or a definition's own name: `Resistor`, `board LedBoard`. */
  | { type: 'type'; name: string }
  /** A string that names a net of the board: `"3V3"`. */
  | { type: 'net'; name: string };

export interface Hit {
  target: Target;
  /** 0-based columns of the token on its line. */
  from: number;
  to: number;
}

/** The target at a 0-based column of a line, or null. `syms` are the file's symbols and `nets` the
 * names of the board's nets. */
export function targetAt(syms: Sym[], text: string, line: number, column: number, nets: ReadonlySet<string>): Hit | null {
  const toks = tokenize(text);
  const at = toks.findIndex((t) => t.from <= column && column < t.to);
  if (at < 0) return null;
  const tok = toks[at];
  const word = text.slice(tok.from, tok.to);
  if (tok.kind === 'string') {
    const name = word.slice(1, -1);
    return nets.has(name) ? { target: { type: 'net', name }, from: tok.from, to: tok.to } : null;
  }
  if (tok.kind === 'type' || tok.kind === 'namespace') {
    // `std::parts::passive::Resistor`: the hovered word of a path names the definition only when it is the last.
    if (tok.kind === 'namespace') return null;
    return { target: { type: 'type', name: word }, from: tok.from, to: tok.to };
  }
  if (tok.kind === 'member') {
    // Only the first member of a path names a declaration of the definition (`.stage` of `.stage.vin`).
    const dot = toks[at - 1]?.kind === 'punct' && text[toks[at - 1].from] === '.';
    const before = toks[at - 2]?.kind;
    const root = dot && before !== 'member' && before !== 'name';
    if (!root) return null;
    const chain = containing(syms, line);
    const owner = chain.length ? chain[0] : null;
    const sym = owner?.children.find((c) => c.name === word) ?? null;
    return sym ? { target: { type: 'decl', sym, owner }, from: tok.from, to: tok.to } : null;
  }
  return null;
}

// ---- the cards ----

export interface PinRow {
  pin: string;
  net: string;
  named: boolean;
  /** The datasheet's name for the pin (`VIN`) and the function the board chose for it, when the server says. */
  name?: string;
  function?: string;
}

export interface PartCard {
  type: 'part';
  refdes: string;
  partKind: string;
  value: string;
  package: string | null;
  path: string;
  /** The modules above the part, as a path (`stage`), or null for a part of the board itself. */
  module: string | null;
  lcsc: string | null;
  assembled: boolean;
  pins: PinRow[];
  decl: Location | null;
  sel: Selection;
  /** The other instances of the same declaration (a module placed twice). */
  also: PartInfo[];
  problems: number;
  /** What the solver worked out for the part, when it has anything to say. */
  solved: Solved | null;
}

export interface RailInfo {
  voltage: string | null;
  supplies: number;
  loads: number;
  demand: string | null;
  capacity: string | null;
  budget: Budget;
}

export interface NetCard {
  type: 'net';
  name: string;
  named: boolean;
  pins: { refdes: string; pin: string; name?: string; function?: string }[];
  /** The modules whose parts are on the net (`stage`), and whether a part of the board itself is. */
  modules: string[];
  onBoard: boolean;
  rail: RailInfo | null;
  sel: Selection;
}

export interface ModuleCard {
  type: 'module';
  path: string;
  /** The definition it is made of (`LdoStage`), when the declaration says. */
  typeName: string | null;
  parts: PartInfo[];
  nets: string[];
  decl: Location | null;
  sel: Selection;
  also: string[];
}

export interface Member {
  name: string;
  typeName?: string;
  role: Role;
  line: number;
}

export interface DefinitionCard {
  type: 'definition';
  name: string;
  defKind: string;
  file: string;
  line: number;
  /** `part Resistor`, the header as written. */
  signature: string;
  doc: string[];
  members: Member[];
  parts: number;
  ports: number;
  errors: number;
  warnings: number;
  library: boolean;
  /** Neither the server's answer nor the text of the file was at hand: members and doc are not known yet. */
  partial: boolean;
  /** Where the signature and the doc come from: the server's `/api/definition`, or the page's reading of the text. */
  source: 'server' | 'text' | 'none';
  /** Why the card is about a definition and not about the thing hovered (`.r1` is not in the board shown). */
  note?: string;
}

export type PartCardData = PartCard;
export type Card = PartCard | NetCard | ModuleCard | DefinitionCard;

export interface CardContext {
  /** The file the pointer is in. */
  path: string;
  detail: BoardDetail | null;
  /** The name of the board the detail is of, for a note. */
  board: string | null;
  project: Project | null;
  problems: Problem[];
  power: PowerTree | null;
  /** The text of a file of the project, when the page has it (open, or fetched for a card). */
  textOf: (file: string) => string | null;
  /** What `/api/definition` answered for the definition the card is about, when it was asked (signature and doc comment). */
  served?: Definition | null;
}

const byPin = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });

/** The pins of a part with the net each is on, in pin order. */
export function pinsOf(detail: BoardDetail, refdes: string): PinRow[] {
  const rows: PinRow[] = [];
  for (const n of detail.nets) {
    for (const p of n.pins) {
      if (p.refdes !== refdes) continue;
      rows.push({ pin: p.pin, net: n.name, named: n.named, ...(p.name ? { name: p.name } : {}), ...(p.function ? { function: p.function } : {}) });
    }
  }
  return rows.sort((a, b) => byPin(a.pin, b.pin));
}

const moduleOf = (path: string): string | null => (path.includes('.') ? path.slice(0, path.lastIndexOf('.')) : null);

/** The kind of the definition a type names, in the project: one in this file first. */
export function kindOfType(project: Project | null, name: string, file?: string): string | undefined {
  return definitionOf(project, name, file)?.def.kind;
}

/** Where a definition is: one in `file` first, else the first in the project. */
export function definitionOf(project: Project | null, name: string, file?: string) {
  let found: { file: string; def: Def; readonly: boolean } | undefined;
  for (const f of project?.files ?? []) {
    const def = f.defs.find((d) => d.name === name);
    if (!def) continue;
    if (f.path === file) return { file: f.path, def, readonly: f.readonly };
    found ??= { file: f.path, def, readonly: f.readonly };
  }
  return found;
}

function problemsIn(problems: Problem[], file: string, from: number, to: number) {
  let errors = 0;
  let warnings = 0;
  for (const p of problems) {
    if (!p.at || p.at.file !== file || p.at.line < from || p.at.line > to) continue;
    if (p.severity === 'error') errors++;
    else if (p.severity === 'warning') warnings++;
  }
  return { errors, warnings };
}

export function partCard(ctx: CardContext, part: PartInfo, also: PartInfo[] = []): PartCard {
  const here = part.decl ? problemsIn(ctx.problems, part.decl.file, part.decl.line, part.decl.endLine) : { errors: 0, warnings: 0 };
  return {
    type: 'part',
    refdes: part.refdes,
    partKind: part.kind,
    value: part.value,
    package: part.package,
    path: part.path,
    module: moduleOf(part.path),
    lcsc: part.lcsc,
    assembled: part.assembled,
    pins: ctx.detail ? pinsOf(ctx.detail, part.refdes) : [],
    decl: part.decl,
    sel: partSel(part.path),
    also,
    problems: here.errors + here.warnings,
    solved: part.solved ?? null,
  };
}

export function netCard(ctx: CardContext, name: string): NetCard | null {
  const net = ctx.detail?.nets.find((n) => n.name === name);
  if (!net || !ctx.detail) return null;
  const modules = new Set<string>();
  let onBoard = false;
  for (const pin of net.pins) {
    const part = ctx.detail.parts.find((p) => p.refdes === pin.refdes);
    const m = part && moduleOf(part.path);
    if (m) modules.add(m.split('.')[0]);
    else onBoard = true;
  }
  const node = ctx.power?.nodes.find((n) => n.net === name);
  return {
    type: 'net',
    name,
    named: net.named,
    pins: net.pins.map((p) => ({ refdes: p.refdes, pin: p.pin, ...(p.name ? { name: p.name } : {}), ...(p.function ? { function: p.function } : {}) })).sort((a, b) => a.refdes.localeCompare(b.refdes, undefined, { numeric: true }) || byPin(a.pin, b.pin)),
    modules: [...modules].sort(),
    onBoard,
    rail: node
      ? {
          voltage: node.voltage?.text ?? null,
          supplies: node.supplies.length,
          loads: node.loads.length,
          demand: node.demand?.text ?? null,
          capacity: node.capacity?.text ?? null,
          budget: node.budget,
        }
      : null,
    sel: netSel(name),
  };
}

export function moduleCard(ctx: CardContext, m: ModuleInfo, typeName: string | null, also: string[] = []): ModuleCard {
  const refs = new Set(m.parts.map((p) => p.refdes));
  const nets = (ctx.detail?.nets ?? []).filter((n) => n.pins.some((p) => refs.has(p.refdes))).map((n) => n.name);
  return { type: 'module', path: m.path, typeName, parts: m.parts, nets, decl: m.decl, sel: moduleSel(m.path), also };
}

/** The role of a declaration from the kind of its type, as the outline gives it. */
const roleOfKind = (k: string | undefined): Role => (k === 'part' ? 'part' : k === 'module' || k === 'board' || k === 'system' || k === 'panel' ? 'module' : k === 'interface' ? 'port' : 'field');

/** The card of a definition by name, from the project's file list (the server's `endLine` and `children` say what it declares and where
 * it ends), `ctx.served` (the server's signature and doc comment) and, for what the server does not say, the text of its file. */
export function definitionCard(ctx: CardContext, name: string, note?: string): DefinitionCard | null {
  const hit = definitionOf(ctx.project, name, ctx.path);
  if (!hit) return null;
  const kindOf = (t: string) => kindOfType(ctx.project, t, hit.file);
  // The server's answer is used when it is about this definition (its place is the one the file list gives) and says something (an older
  // server answers with the place alone).
  const answer = ctx.served && ctx.served.at.file === hit.file && ctx.served.at.line === hit.def.line ? ctx.served : null;
  const served = answer && (answer.signature !== undefined || answer.doc !== undefined) ? answer : null;
  // The file list of a newer server says where each definition ends and what it declares; the text is read only for what it does not say.
  const fromServer = hit.def.endLine !== undefined;
  const fileText = ctx.textOf(hit.file);
  const lines = fileText?.split('\n') ?? [];
  const sym = !fromServer && fileText ? flatten(scanSymbols(fileText)).find((s) => s.kind !== 'decl' && s.name === name && s.line === hit.def.line) : undefined;
  const members: Member[] = fromServer
    ? (hit.def.children ?? []).map((c) => ({ name: c.name, typeName: c.typeName, role: c.kind ? roleOfKind(c.kind) : roleOfKind(kindOf(c.typeName)), line: c.line }))
    : (sym?.children ?? []).map((c) => ({ name: c.name, typeName: c.typeName, role: roleOf(c, kindOf), line: c.line }));
  const problems = problemsIn(ctx.problems, hit.file, hit.def.line, hit.def.endLine ?? sym?.endLine ?? hit.def.line);
  const doc = served ? (served.doc ?? '').split('\n').filter((l, _i, all) => all.length > 1 || l) : fileText ? docAbove(lines, hit.def.line) : [];
  return {
    type: 'definition',
    name,
    defKind: hit.def.kind,
    file: hit.file,
    line: hit.def.line,
    signature: served?.signature ?? `${hit.def.kind} ${name}`,
    doc,
    members,
    parts: members.filter((m) => m.role === 'part').length,
    ports: members.filter((m) => m.role === 'port').length,
    errors: problems.errors,
    warnings: problems.warnings,
    library: hit.readonly,
    // Members and ends are known from the file list of a server that sends them; the doc comment then comes with the answer of
    // `/api/definition`, or from the text.
    partial: !served && !fileText && !fromServer,
    source: served ? 'server' : fileText ? 'text' : 'none',
    note,
  };
}

/** The card for what a target stands for, or null when nothing is known of it. */
export function cardFor(ctx: CardContext, target: Target): Card | null {
  if (target.type === 'net') return netCard(ctx, target.name);
  if (target.type === 'type') return definitionCard(ctx, target.name);
  const { sym } = target;
  const detail = ctx.detail;
  if (detail) {
    const parts = detail.parts.filter((p) => p.decl && p.decl.file === ctx.path && p.decl.line === sym.line);
    if (parts.length) return partCard(ctx, parts[0], parts.slice(1));
    const mods = modulesOf(detail).filter((m) => m.decl && m.decl.file === ctx.path && m.decl.line === sym.line);
    if (mods.length) return moduleCard(ctx, mods[0], sym.typeName ?? null, mods.slice(1).map((m) => m.path));
  }
  if (!sym.typeName) return null;
  const why = ctx.board ? `\`.${sym.name}\` is not in ${ctx.board}, the board the views show.` : `\`.${sym.name}\` is not built into a board yet.`;
  return definitionCard(ctx, sym.typeName, why);
}

// ---- what the solver worked out for a part ----

export interface SolvedLine {
  label: string;
  value: string;
  mono?: boolean;
}

/** A range as the page writes it (`Range.text` has no space before the unit). */
export const rangeText = (r: Range): string => spaceUnits(r.text);

/** The rows of a part's `solved` block, in the order a person reads them: what it is worth in use, its tolerance, what it is rated for and
 * the catalog row that stands for it. Empty when the solver had nothing to say. */
export function solvedRows(solved: Solved | null | undefined): SolvedLine[] {
  if (!solved) return [];
  const rows: SolvedLine[] = [];
  if (solved.effectiveCapacitance) {
    rows.push({ label: 'In use', value: `${rangeText(solved.effectiveCapacitance)}${solved.bias ? ` at ${rangeText(solved.bias)}` : ''}`, mono: true });
  }
  if (solved.tolerance) rows.push({ label: 'Tolerance', value: spaceUnits(solved.tolerance.text) });
  if (solved.ratings?.length) rows.push({ label: 'Rated', value: solved.ratings.map((r) => `${r.name.replace(/_/g, ' ')} ${rangeText(r.range)}`).join(', ') });
  const row = solved.row;
  if (row) {
    const id = [row.mpn, row.lcsc, row.library].filter(Boolean).join(' · ');
    rows.push({
      label: 'Row',
      value: row.generic ? `generic${row.catalog ? ` ${row.catalog}` : ''}: a value and a package, no catalog row yet` : [row.catalog, id].filter(Boolean).join(' · '),
      mono: !row.generic,
    });
  }
  return rows;
}

// ---- a declaration of a module that several boards use ----

/** The boards that use the module a declaration is written in, by the file and line of the module's definition (`Analysis.moduleUses`).
 * Empty when the declaration is not inside a definition of a module that a board uses. */
export function boardsUsing(uses: ModuleUse[] | null | undefined, file: string, definitionLine: number | null): string[] {
  if (!uses || definitionLine === null) return [];
  return uses.find((u) => u.file === file && u.line === definitionLine)?.usedBy ?? [];
}

/** Whether the card offers a switch of board: the module is shared by several boards, or the one shown does not hold it. */
export function offersBoards(boards: string[], shown: string | null): boolean {
  return boards.length > 1 || (boards.length === 1 && boards[0] !== shown);
}

/** The module uses of an analysis, for `boardsUsing`. */
export const usesOf = (a: Pick<Analysis, 'moduleUses'> | null | undefined): ModuleUse[] | null => a?.moduleUses ?? null;
