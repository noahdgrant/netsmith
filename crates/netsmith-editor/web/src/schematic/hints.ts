// The schematic layout file as the page edits it (ADR 0061): pure functions over what
// `GET /api/schematic-hints` says (`SchematicHints.boards`), and the text the page sends back.
//
// The server reads and writes the file (`PUT` takes the TOML text and writes its canonical form), so the page
// never needs a TOML reader: it has the tables as data. It does need a writer, small, because the route takes
// text. The writer spells numbers and keys as the server's does, so what the server writes back is the same text.
//
// A part is keyed by its path from its sheet, a sheet being the top-level module instance the part is in (the first
// word of its path), or the board's own sheet for a part the board declares itself.

import type { HintBoard, HintNote, HintPart, HintSheet } from '../api';

/** The grid of the schematic in millimetres: every offset of the file is a multiple of it. */
export const GRID_MM = 1.27;

/** A multiple of the grid, as a number with the digits the file spells (`3.81`, never `3.8099999999999996`). */
export const stepsToMm = (steps: number): number => Math.round(steps * 127) / 100 + 0;

/** The number of grid steps nearest to `mm`. */
export const mmToSteps = (mm: number): number => Math.round(mm / GRID_MM) + 0;

/** An entry of a part as the page edits it: no `line` (where it is in the file is the file's business). */
export type PartEntry = Omit<HintPart, 'line'>;
export type NoteEntry = Omit<HintNote, 'line'>;

export interface SheetTables {
  parts: Record<string, PartEntry>;
  notes: Record<string, NoteEntry>;
}
export interface BoardTables extends SheetTables {
  sheets: Record<string, SheetTables>;
}
/** The tables of the whole file by the boards' definition names. */
export type Tables = Record<string, BoardTables>;

/** What a part is to the file: its sheet (`null` for the board's own) and its key from the sheet. */
export interface HintAddress {
  sheet: string | null;
  key: string;
}

/** The address of the part at `path` (from the board: `stage.filter.l1`). */
export function addressOf(path: string): HintAddress {
  const at = path.indexOf('.');
  return at < 0 ? { sheet: null, key: path } : { sheet: path.slice(0, at), key: path.slice(at + 1) };
}

/** A path's key from the sheet it is drawn on (`filter.l1` for `stage.filter.l1` on `stage`; the path itself on the board's own
 * sheet, `null`; empty for the module instance itself). It is `netsmith_view::hints::sheet_key`. */
export function keyFrom(sheet: string | null, path: string): string {
  if (sheet === null) return path;
  if (path === sheet) return '';
  return path.startsWith(`${sheet}.`) ? path.slice(sheet.length + 1) : path;
}

/** The address of a note: the sheet it is drawn on (`null`: the board's own) and `<path from the sheet>#doc`, `#relation`, or
 * `#relation2` for the second relation of one path. `nth` counts the notes of that path and kind in the order the design lists
 * them, from 1. */
export function noteAddress(sheet: string | null, path: string, kind: 'doc' | 'relation', nth: number): HintAddress {
  return { sheet, key: `${keyFrom(sheet, path)}#${kind}${kind === 'relation' && nth > 1 ? nth : ''}` };
}

const strip = <T extends { line: number }>(e: T): Omit<T, 'line'> => {
  const { line: _line, ...rest } = e;
  return rest;
};

const sheetOf = (s: HintSheet | HintBoard): SheetTables => ({
  parts: Object.fromEntries(Object.entries(s.parts).map(([k, v]) => [k, strip(v)])),
  notes: Object.fromEntries(Object.entries(s.notes).map(([k, v]) => [k, strip(v)])),
});

/** The tables as the page edits them, from what the server read (a copy: nothing is shared with it). */
export function tablesOf(boards: Record<string, HintBoard>): Tables {
  const out: Tables = {};
  for (const [name, b] of Object.entries(boards)) {
    out[name] = {
      ...sheetOf(b),
      sheets: Object.fromEntries(Object.entries(b.sheets).map(([s, t]) => [s, sheetOf(t)])),
    };
  }
  return out;
}

const emptySheet = (): SheetTables => ({ parts: {}, notes: {} });

function tableOf(tables: Tables, board: string, sheet: string | null): SheetTables | undefined {
  const b = tables[board];
  if (!b) return undefined;
  return sheet === null ? b : b.sheets[sheet];
}

/** The entry of a part, if the file has one. */
export const partEntry = (tables: Tables, board: string, at: HintAddress): PartEntry | undefined => tableOf(tables, board, at.sheet)?.parts[at.key];
export const noteEntry = (tables: Tables, board: string, at: HintAddress): NoteEntry | undefined => tableOf(tables, board, at.sheet)?.notes[at.key];

/** The tables with the part's entry set (`null` removes it). Nothing is changed in `tables`. An entry with no field is kept
 * only when asked (`{}`): a nudge that comes back to the start is an entry that says nothing, and is removed. */
export function withPart(tables: Tables, board: string, at: HintAddress, entry: PartEntry | null): Tables {
  const next: Tables = structuredClone(tables);
  const b = (next[board] ??= { ...emptySheet(), sheets: {} });
  const t = at.sheet === null ? b : (b.sheets[at.sheet] ??= emptySheet());
  if (entry === null || isBlank(entry)) delete t.parts[at.key];
  else t.parts[at.key] = entry;
  return tidy(next);
}

export function withNote(tables: Tables, board: string, at: HintAddress, entry: NoteEntry | null): Tables {
  const next: Tables = structuredClone(tables);
  const b = (next[board] ??= { ...emptySheet(), sheets: {} });
  const t = at.sheet === null ? b : (b.sheets[at.sheet] ??= emptySheet());
  if (entry === null || (entry.dx === 0 && entry.dy === 0)) delete t.notes[at.key];
  else t.notes[at.key] = entry;
  return tidy(next);
}

/** An entry that changes nothing: no offset, no pin, no turn, no mirror. */
export function isBlank(e: PartEntry): boolean {
  return !e.dx && !e.dy && e.pin === undefined && e.rotate === undefined && e.mirror === undefined;
}

/** Drop the empty tables, so the file keeps no header that holds nothing. */
function tidy(tables: Tables): Tables {
  for (const [name, b] of Object.entries(tables)) {
    for (const [s, t] of Object.entries(b.sheets)) {
      if (Object.keys(t.parts).length === 0 && Object.keys(t.notes).length === 0) delete b.sheets[s];
    }
    if (Object.keys(b.parts).length === 0 && Object.keys(b.notes).length === 0 && Object.keys(b.sheets).length === 0) delete tables[name];
  }
  return tables;
}

/** How many entries (parts and notes) a sheet has; `null` is the board's own sheet. */
export function countIn(tables: Tables, board: string, sheet: string | null): number {
  const t = tableOf(tables, board, sheet);
  return t ? Object.keys(t.parts).length + Object.keys(t.notes).length : 0;
}

/** How many entries the whole file has, in every board. */
export function countAll(tables: Tables): number {
  let n = 0;
  for (const [name, b] of Object.entries(tables)) {
    n += countIn(tables, name, null);
    for (const s of Object.keys(b.sheets)) n += countIn(tables, name, s);
  }
  return n;
}

/** The tables without the entries of a sheet of the board (`null` for the board's own sheet). */
export function withoutSheet(tables: Tables, board: string, sheet: string | null): Tables {
  const next: Tables = structuredClone(tables);
  const b = next[board];
  if (!b) return next;
  if (sheet === null) {
    b.parts = {};
    b.notes = {};
  } else delete b.sheets[sheet];
  return tidy(next);
}

/** The tables of a file with nothing in it. */
export const NOTHING: Tables = {};

// ------------------------------------------------------------------ the text

const BARE = /^[A-Za-z0-9_-]+$/;
/** A key as TOML spells it: bare when it can be, else in quotes. */
export function keyText(key: string): string {
  return BARE.test(key) ? key : JSON.stringify(key);
}

/** A number as the file spells it: no exponent, no trailing zeros, no `-0`. */
export function numberText(n: number): string {
  const s = (Math.round(n * 1000) / 1000).toFixed(3).replace(/\.?0+$/, '');
  return s === '-0' || s === '' ? '0' : s;
}

function partLines(header: string, e: PartEntry): string[] {
  const lines = [`[${header}]`];
  if (e.pin) lines.push(`pin = [${numberText(e.pin[0])}, ${numberText(e.pin[1])}]`);
  else {
    if (e.dx !== undefined) lines.push(`dx = ${numberText(e.dx)}`);
    if (e.dy !== undefined) lines.push(`dy = ${numberText(e.dy)}`);
  }
  if (e.rotate !== undefined) lines.push(`rotate = ${e.rotate}`);
  if (e.mirror !== undefined) lines.push(`mirror = ${e.mirror}`);
  return lines;
}

/** The text of the file for the tables: `version = 1`, then each board, its own sheet's parts and notes, then the sheets
 * in name order. The server reads it and writes its own canonical text, which is this one. */
export function toToml(tables: Tables): string {
  const out: string[] = ['version = 1'];
  const sorted = (o: Record<string, unknown>) => Object.keys(o).sort();
  const table = (prefix: string, t: SheetTables) => {
    for (const k of sorted(t.parts)) out.push('', ...partLines(`${prefix}.parts.${keyText(k)}`, t.parts[k]));
    for (const k of sorted(t.notes)) {
      const n = t.notes[k];
      out.push('', `[${prefix}.notes.${keyText(k)}]`, `dx = ${numberText(n.dx)}`, `dy = ${numberText(n.dy)}`);
    }
  };
  for (const name of sorted(tables)) {
    const b = tables[name];
    table(keyText(name), b);
    for (const s of sorted(b.sheets)) table(`${keyText(name)}.sheets.${keyText(s)}`, b.sheets[s]);
  }
  return `${out.join('\n')}\n`;
}
