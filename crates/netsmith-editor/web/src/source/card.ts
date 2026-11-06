// What a hover card in the source says. Two steps, both pure: `targetAt` reads the token under the
// pointer (a declaration's name, a type, a string that names a net) from the line and the file's
// symbols; `cardFor` builds the card from what the page knows of the board (`BoardDetail`, the power
// tree, the project's files) and, for a definition, the text of its file. Nothing is fetched here:
// a card that needs a file's text or the power tree is built without it and again when it is there.

import type { Budget, BoardDetail, Location, PartInfo, PowerTree, Problem, Project } from '../api';
import { tokenize } from '../nsm';
import { type Selection, type ModuleInfo, modulesOf, netSel, partSel, moduleSel } from '../selection';
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
  pins: { refdes: string; pin: string }[];
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
  /** The text of the file was not at hand: members and doc are not known yet. */
  partial: boolean;
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
}

const byPin = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });

/** The pins of a part with the net each is on, in pin order. */
export function pinsOf(detail: BoardDetail, refdes: string): PinRow[] {
  const rows: PinRow[] = [];
  for (const n of detail.nets) for (const p of n.pins) if (p.refdes === refdes) rows.push({ pin: p.pin, net: n.name, named: n.named });
  return rows.sort((a, b) => byPin(a.pin, b.pin));
}

const moduleOf = (path: string): string | null => (path.includes('.') ? path.slice(0, path.lastIndexOf('.')) : null);

/** The kind of the definition a type names, in the project: one in this file first. */
export function kindOfType(project: Project | null, name: string, file?: string): string | undefined {
  return definitionOf(project, name, file)?.def.kind;
}

/** Where a definition is: one in `file` first, else the first in the project. */
export function definitionOf(project: Project | null, name: string, file?: string) {
  let found: { file: string; def: { kind: string; name: string; line: number }; readonly: boolean } | undefined;
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
    pins: net.pins.map((p) => ({ refdes: p.refdes, pin: p.pin })).sort((a, b) => a.refdes.localeCompare(b.refdes, undefined, { numeric: true }) || byPin(a.pin, b.pin)),
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

/** The card of a definition by name, from the project's file list and the text of its file when known. */
export function definitionCard(ctx: CardContext, name: string, note?: string): DefinitionCard | null {
  const hit = definitionOf(ctx.project, name, ctx.path);
  if (!hit) return null;
  const text = ctx.textOf(hit.file);
  const lines = text?.split('\n') ?? [];
  const sym = text ? flatten(scanSymbols(text)).find((s) => s.kind !== 'decl' && s.name === name && s.line === hit.def.line) : undefined;
  const kindOf = (t: string) => kindOfType(ctx.project, t, hit.file);
  const members: Member[] = (sym?.children ?? []).map((c) => ({ name: c.name, typeName: c.typeName, role: roleOf(c, kindOf), line: c.line }));
  const problems = problemsIn(ctx.problems, hit.file, hit.def.line, sym?.endLine ?? hit.def.line);
  return {
    type: 'definition',
    name,
    defKind: hit.def.kind,
    file: hit.file,
    line: hit.def.line,
    signature: `${hit.def.kind} ${name}`,
    doc: text ? docAbove(lines, hit.def.line) : [],
    members,
    parts: members.filter((m) => m.role === 'part').length,
    ports: members.filter((m) => m.role === 'port').length,
    errors: problems.errors,
    warnings: problems.warnings,
    library: hit.readonly,
    partial: !text,
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
