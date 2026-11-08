// The rows of the problems dock: what the checker, the solver and the PCB checks found, as one
// list that can be sorted, filtered and stepped through. Pure: no signals, no DOM.
//
// The server tells a problem's stage, board and instance (`Problem.stage`, `.board`, `.path`) and
// the rows read them. `classify`, `splitBoard` and `instancePath` read the same from the message,
// and are used only for a problem that has no `stage`: a server older than the fields.

import type { Location, PcbFinding, Problem, ProblemStage, Severity } from '../api';
import type { Section } from '../layout';

export type Origin = 'syntax' | 'checker' | 'solver' | 'lower' | 'guideline' | 'layout' | 'power' | 'placement' | 'routing' | 'rules' | 'kicad';

export const ORIGIN_LABEL: Record<Origin, string> = {
  syntax: 'Syntax',
  checker: 'Checker',
  solver: 'Solver',
  lower: 'Lowering',
  guideline: 'Guideline',
  layout: 'Layout',
  power: 'Power',
  placement: 'Placement',
  routing: 'Routing',
  rules: 'Design rules',
  kicad: 'KiCad',
};

/** One line of the dock. */
export interface Row {
  /** Stable across re-reads of the same problem, so the picked row survives a rebuild. */
  id: string;
  severity: Severity;
  /** The first line of the message, without the board's name in front. */
  message: string;
  /** The whole message, for the tooltip and the details. */
  full: string;
  origin: Origin;
  at: Location | null;
  /** The board the message is about, when it says (`LedExample: ...`). */
  board: string | null;
  /** The instance path inside the board (`status_indicator.led`), when the message names one. */
  path: string | null;
  /** The section the finding belongs to: a PCB finding is the PCB view's. */
  home: Section | null;
  /** A PCB finding, for the click that selects it on the board. */
  pcb?: PcbFinding;
  /** The power issue's net, for the click that opens the power tree. */
  net?: string;
  /** Where the message says the thing is, when it is not a place the server located (a
   * requirement in a physical file that a PCB finding names). */
  mentions?: string;
}

const SEVERITY_RANK: Record<Severity, number> = { error: 0, warning: 1, note: 2 };
export const severityRank = (s: Severity) => SEVERITY_RANK[s];

const SYNTAX = /^(expected\b|unexpected character|unknown escape|invalid number|a pin table takes|this .* is never closed|this doc comment)/;

/** The board label a build's message starts with (`Led: not evaluated ...`), and the rest. */
export function splitBoard(message: string): { board: string | null; rest: string } {
  const m = /^([A-Za-z_][\w]*(?:\/[A-Za-z_]\w*)?): (.*)$/s.exec(message);
  return m ? { board: m[1], rest: m[2] } : { board: null, rest: message };
}

/** The first dotted name in backticks: the instance a solver message is about. */
export function instancePath(text: string): string | null {
  for (const m of text.matchAll(/`([A-Za-z_][\w]*(?:\.[A-Za-z_]\w*)+)`/g)) return m[1];
  return null;
}

/** Which stage of the build a problem came from, by what its message says. */
export function classify(problem: Pick<Problem, 'severity' | 'message'>): Origin {
  const { rest } = splitBoard(problem.message);
  if (SYNTAX.test(problem.message)) return 'syntax';
  if (/\bguideline:/.test(rest)) return 'guideline';
  if (problem.severity !== 'error' && /(^not evaluated\b|: not evaluated\b|^not chosen\b|: not chosen\b|is not known|balance|not settled|incomplete)/.test(rest)) {
    return 'solver';
  }
  if (/\bnot chosen\b|\bno part\b|\bnot evaluated\b|\bsolver\b|\bcontradict/.test(rest)) return 'solver';
  return 'checker';
}

const first = (text: string) => text.split('\n', 1)[0];

/** The stage the server names, as the dock's origin. */
export const ORIGIN_OF_STAGE: Record<ProblemStage, Origin> = {
  syntax: 'syntax',
  check: 'checker',
  solve: 'solver',
  lower: 'lower',
  guideline: 'guideline',
  power: 'power',
  layout: 'layout',
};

/** The stage, the board, the instance and the message without the board's label of a problem:
 * the server's fields when it sent a stage, else what the message says. A server that sends the
 * stage and no board means the problem has none, so the message is not read for one. */
export function describeProblem(p: Problem): { origin: Origin; board: string | null; path: string | null; rest: string } {
  if (p.stage === undefined) {
    const { board, rest } = splitBoard(p.message);
    return { origin: classify(p), board, path: instancePath(p.message), rest };
  }
  const board = p.board ?? null;
  const label = board === null ? '' : `${board}: `;
  return {
    origin: ORIGIN_OF_STAGE[p.stage] ?? 'checker',
    board,
    path: p.path ?? null,
    rest: label && p.message.startsWith(label) ? p.message.slice(label.length) : p.message,
  };
}

export function rowOfProblem(p: Problem, index: number): Row {
  const { board, rest, origin, path } = describeProblem(p);
  const at = p.at;
  // A power problem is about a rail or a converter, which the power view shows: the row opens it.
  const power = origin === 'power';
  return {
    id: `p|${p.severity}|${at ? `${at.file}:${at.line}:${at.column}` : '-'}|${first(p.message)}|${index}`,
    severity: p.severity,
    message: first(rest),
    full: p.message,
    origin,
    at,
    board,
    path,
    home: power ? 'power' : null,
    ...(power && path ? { net: path } : {}),
  };
}

const PCB_ORIGIN: Record<PcbFinding['source'], Origin> = { placement: 'placement', unrouted: 'routing', drc: 'rules', kicad: 'kicad' };

/** A `file.nsm:12` that a finding's message names: where its requirement is written. */
export function mentionedPlace(message: string): string | undefined {
  return /\(([\w./-]+\.\w+:\d+)\)/.exec(message)?.[1];
}

export function rowOfFinding(f: PcbFinding, board: string): Row {
  const at = f.at ?? null;
  return {
    // The line the server found; the message is read for it only when the server did not say.
    mentions: at ? undefined : mentionedPlace(f.message),
    id: `f|${board}|${f.id}`,
    severity: f.severity,
    message: first(f.message),
    full: f.message,
    origin: PCB_ORIGIN[f.source],
    at,
    board,
    path: f.parts.length ? f.parts.join(', ') : null,
    home: 'pcb',
    pcb: f,
  };
}

export type Group = 'none' | 'file' | 'severity' | 'stage';

/** A stored grouping that is one of the known ones, else none (the dock kept it in the browser: an older or newer page may have written another). */
export function parseGroup(v: unknown): Group {
  return v === 'file' || v === 'severity' || v === 'stage' ? v : 'none';
}

export interface Filter {
  errors: boolean;
  warnings: boolean;
  notes: boolean;
  /** Only what belongs to the section on screen. */
  thisView: boolean;
  /** Words that must all be in the message, the file or the path. */
  text: string;
  /** The stages whose chips are off: their rows are left out. */
  off: Origin[];
}

export const DEFAULT_FILTER: Filter = { errors: true, warnings: true, notes: false, thisView: false, text: '', off: [] };

/** The stages in the order their chips come: the compiler's in the order of the pipeline, then what the views find. */
export const ORIGIN_ORDER: Origin[] = ['syntax', 'checker', 'solver', 'lower', 'guideline', 'layout', 'power', 'placement', 'routing', 'rules', 'kicad'];

/** The stages that have rows, each with its count, in chip order. */
export function stagesOf(rows: Row[]): { origin: Origin; n: number }[] {
  const n = new Map<Origin, number>();
  for (const r of rows) n.set(r.origin, (n.get(r.origin) ?? 0) + 1);
  return ORIGIN_ORDER.filter((o) => n.has(o)).map((origin) => ({ origin, n: n.get(origin)! }));
}

/** The file a row is about, or `''` for one that has none. */
export const fileOf = (r: Row) => r.at?.file ?? '';

/** Errors first, then warnings, then notes; within a severity syntax errors first, then by file,
 * line and column, then what the message says. A row without a place goes after those with one. */
export function compareRows(a: Row, b: Row): number {
  const s = severityRank(a.severity) - severityRank(b.severity);
  if (s) return s;
  // A mistake in the syntax comes first: the checker's complaints that follow from it are its
  // consequences (a module that "has no definition" because its file did not parse).
  const syntax = Number(b.origin === 'syntax') - Number(a.origin === 'syntax');
  if (syntax) return syntax;
  const ha = a.at ? 0 : 1;
  const hb = b.at ? 0 : 1;
  if (ha !== hb) return ha - hb;
  if (a.at && b.at) {
    const f = a.at.file.localeCompare(b.at.file);
    if (f) return f;
    if (a.at.line !== b.at.line) return a.at.line - b.at.line;
    if (a.at.column !== b.at.column) return a.at.column - b.at.column;
  }
  return a.message.localeCompare(b.message) || a.id.localeCompare(b.id);
}

export const sortRows = (rows: Row[]): Row[] => [...rows].sort(compareRows);

/** What the section on screen is about, for "this view only". */
export interface ViewScope {
  section: Section;
  /** The file open in the source. */
  file: string | null;
  /** The board the views show. */
  board: string | null;
}

/** Whether a row belongs to what the section shows: the PCB view shows the PCB's findings, the
 * source shows the problems of its open file (and those with no place), the other views show what
 * is about their board. */
export function inScope(r: Row, view: ViewScope): boolean {
  if (view.section === 'pcb') return r.home === 'pcb' || (r.home === null && sameBoard(r, view.board));
  if (r.home === 'pcb') return false;
  if (view.section === 'source') return r.at ? r.at.file === view.file : true;
  if (view.section === 'power') return r.home === 'power' || (r.home === null && sameBoard(r, view.board));
  return r.home === null && sameBoard(r, view.board);
}

/** A board's own label reads `Led`; in a system it is `Sys/member`. A message with no board is
 * about the project. */
function sameBoard(r: Row, board: string | null): boolean {
  return r.board === null || board === null || r.board === board;
}

export function filterRows(rows: Row[], f: Filter, view: ViewScope): Row[] {
  const words = f.text.toLowerCase().split(/\s+/).filter(Boolean);
  return rows.filter((r) => {
    if (r.severity === 'error' ? !f.errors : r.severity === 'warning' ? !f.warnings : !f.notes) return false;
    if (f.off?.includes(r.origin)) return false;
    if (f.thisView && !inScope(r, view)) return false;
    if (words.length === 0) return true;
    const hay = `${r.message} ${r.at?.file ?? ''} ${r.path ?? ''} ${ORIGIN_LABEL[r.origin]} ${r.board ?? ''}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

export interface Counts {
  error: number;
  warning: number;
  note: number;
}

export function countRows(rows: Row[]): Counts {
  const c: Counts = { error: 0, warning: 0, note: 0 };
  for (const r of rows) c[r.severity]++;
  return c;
}

export interface RowGroup {
  key: string;
  title: string;
  rows: Row[];
}

const SEVERITY_TITLE: Record<Severity, string> = { error: 'Errors', warning: 'Warnings', note: 'Notes' };

/** The rows, in the order the dock shows them, with the heads of their groups. Severities and stages come in a fixed order (errors, warnings,
 * notes; the stages as their chips are), so a group does not move when the list changes; files come in the order of their first row, so a
 * file with an error comes before one with a note. */
export function groupRows(rows: Row[], by: Group): RowGroup[] {
  const sorted = sortRows(rows);
  if (by === 'none') return [{ key: '', title: '', rows: sorted }];
  const map = new Map<string, RowGroup>();
  for (const r of sorted) {
    const key = by === 'file' ? fileOf(r) || '(no file)' : by === 'stage' ? r.origin : r.severity;
    const title = by === 'file' ? key : by === 'stage' ? ORIGIN_LABEL[r.origin] : SEVERITY_TITLE[r.severity];
    if (!map.has(key)) map.set(key, { key, title, rows: [] });
    map.get(key)!.rows.push(r);
  }
  const groups = [...map.values()];
  if (by === 'stage') groups.sort((a, b) => ORIGIN_ORDER.indexOf(a.key as Origin) - ORIGIN_ORDER.indexOf(b.key as Origin));
  return groups;
}

/** Where a row points in the source: the place the server gave, else the `file.nsm:12` its message names. Null for a row with neither. */
export function locationOf(r: Row): Location | null {
  if (r.at) return r.at;
  const m = r.mentions ? /^(.*):(\d+)$/.exec(r.mentions) : null;
  return m ? { file: m[1], line: Number(m[2]), column: 1, endLine: Number(m[2]), endColumn: 1 } : null;
}

/** The rows in the order the dock lists them (what F8 steps through): notes are left out unless
 * they are all there is to step through. */
export function steps(rows: Row[], by: Group): Row[] {
  const all = groupRows(rows, by).flatMap((g) => g.rows);
  const real = all.filter((r) => r.severity !== 'note');
  return real.length ? real : all;
}

/** Where F8 starts from when nothing in the list is picked: the cursor in the source. */
export interface Cursor {
  file: string;
  line: number;
  column: number;
}

/** The row after (`dir` 1) or before (`dir` -1) the picked one, wrapping at the ends. With none
 * picked, it is the first row after the cursor (or the last before it); with no cursor either, the
 * first (or last) row. `null` when there is nothing to step to. */
export function nextRow(list: Row[], pickedId: string | null, cursor: Cursor | null, dir: 1 | -1): Row | null {
  if (list.length === 0) return null;
  const at = pickedId ? list.findIndex((r) => r.id === pickedId) : -1;
  if (at >= 0) return list[(at + dir + list.length) % list.length];
  if (cursor) {
    const before = (r: Row) =>
      !!r.at && r.at.file === cursor.file && (r.at.line < cursor.line || (r.at.line === cursor.line && r.at.column <= cursor.column));
    const inFile = list.filter((r) => r.at?.file === cursor.file);
    if (inFile.length) {
      if (dir === 1) return inFile.find((r) => !before(r)) ?? list[0];
      const earlier = inFile.filter((r) => before(r) && !(r.at!.line === cursor.line && r.at!.column === cursor.column));
      return earlier[earlier.length - 1] ?? list[list.length - 1];
    }
  }
  return dir === 1 ? list[0] : list[list.length - 1];
}

/** `file:line` as the list prints it; `file` alone when the line is not known. */
export function whereText(r: Row): string {
  return r.at ? `${r.at.file}:${r.at.line}` : (r.mentions ?? '');
}

/** The summary the header and the status bar print: `2 errors, 1 warning`; `No problems`. */
export function summaryText(c: Counts): string {
  const parts: string[] = [];
  if (c.error) parts.push(`${c.error} ${c.error === 1 ? 'error' : 'errors'}`);
  if (c.warning) parts.push(`${c.warning} ${c.warning === 1 ? 'warning' : 'warnings'}`);
  if (!parts.length) return 'No problems';
  return parts.join(', ');
}
