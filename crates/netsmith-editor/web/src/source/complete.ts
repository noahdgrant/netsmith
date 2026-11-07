// What the editor offers while typing, chosen from the line before the cursor: the members declared
// in the definition after a dot, the types of the project and the library after `name:`, the units
// after a number, the names of the board's nets inside the quotes of `.net &= "`, and the keywords
// at the start of a line. Pure: the extension (`cm-complete.ts`) hands in the page's data.

import type { BoardDetail, Project } from '../api';
import { KEYWORDS } from '../nsm';
import { type Sym, containing, flatten, scanSymbols } from './symbols';

export type CompletionKind = 'part' | 'module' | 'interface' | 'net' | 'keyword' | 'unit' | 'field';

export interface Option {
  label: string;
  kind: CompletionKind;
  detail?: string;
  /** Higher sorts first among equal matches. */
  boost?: number;
}

export interface Offer {
  /** 0-based column where the replaced word starts. */
  from: number;
  options: Option[];
}

export interface CompletionData {
  syms: Sym[];
  line: number;
  project: Project | null;
  detail: BoardDetail | null;
  /** The text of a file of the project, when the page has it. */
  textOf: (file: string) => string | null;
}

/** The units the language reads (docs/LANGUAGE.md, Values), with the quantity each measures. */
export const UNITS: [string, string][] = [
  ['V', 'voltage'], ['mV', 'millivolt'], ['uV', 'microvolt'], ['kV', 'kilovolt'],
  ['A', 'current'], ['mA', 'milliamp'], ['uA', 'microamp'], ['nA', 'nanoamp'],
  ['ohm', 'resistance'], ['kohm', 'kilo-ohm'], ['Mohm', 'megohm'], ['mohm', 'milli-ohm'],
  ['F', 'capacitance'], ['uF', 'microfarad'], ['nF', 'nanofarad'], ['pF', 'picofarad'],
  ['H', 'inductance'], ['uH', 'microhenry'], ['nH', 'nanohenry'], ['mH', 'millihenry'],
  ['Hz', 'frequency'], ['kHz', 'kilohertz'], ['MHz', 'megahertz'], ['GHz', 'gigahertz'],
  ['W', 'power'], ['mW', 'milliwatt'],
  ['m', 'length'], ['mm', 'millimetre'], ['g', 'mass'], ['s', 'time'], ['ms', 'millisecond'], ['us', 'microsecond'], ['K', 'temperature'], ['%', 'tolerance'],
];

const WORD = '[\\p{L}\\p{N}_]';
// A generic is a type to declare, as a part is (ADR 0068): `.r1: Resistor`.
const kindOfDef = (k: string): CompletionKind | null => (k === 'part' || k === 'generic' ? 'part' : k === 'module' || k === 'interface' ? k : null);

/** The type words of the project: parts, modules and interfaces, the project's own first. */
export function typeOptions(project: Project | null): Option[] {
  const out: Option[] = [];
  const seen = new Set<string>();
  for (const f of project?.files ?? []) {
    for (const d of f.defs) {
      const kind = kindOfDef(d.kind);
      if (!kind || seen.has(d.name)) continue;
      seen.add(d.name);
      out.push({ label: d.name, kind, detail: f.role === 'std' ? `${d.kind}, library` : d.kind, boost: f.role === 'std' ? 0 : 2 });
    }
  }
  return out;
}

function memberOptions(data: CompletionData, path: string[]): Option[] {
  const owner = containing(data.syms, data.line)[0];
  if (!owner) return [];
  // `.a.` offers what the definition of `.a`'s type declares; `.` offers what this definition declares.
  if (path.length === 0) {
    return owner.children.map((c) => ({ label: c.name, kind: 'field' as const, detail: c.typeName, boost: 1 }));
  }
  const decl = owner.children.find((c) => c.name === path[0]);
  if (!decl?.typeName) return [];
  const where = data.project?.files.find((f) => f.defs.some((d) => d.name === decl.typeName));
  const text = where ? data.textOf(where.path) : null;
  if (!text) return [];
  const def = flatten(scanSymbols(text)).find((s) => s.kind !== 'decl' && s.name === decl.typeName);
  return (def?.children ?? []).map((c) => ({ label: c.name, kind: 'field' as const, detail: c.typeName }));
}

/** What to offer at a 0-based column of a line, or null. `explicit` is a request for completions (Ctrl+Space). */
export function offerAt(data: CompletionData, text: string, col: number, explicit = false): Offer | null {
  const before = text.slice(0, col);
  // Inside a comment: nothing.
  const comment = before.indexOf('//');
  if (comment >= 0 && (before.slice(0, comment).match(/"/g)?.length ?? 0) % 2 === 0) return null;
  // The name of a net between quotes: `.pos.net &= "3`.
  const net = new RegExp(`\\.net\\s*&?=\\s*"(${WORD}*)$`, 'u').exec(before);
  if (net) {
    return {
      from: col - net[1].length,
      options: (data.detail?.nets ?? []).filter((n) => n.named).map((n) => ({ label: n.name, kind: 'net' as const, detail: `net, ${n.pins.length} pin${n.pins.length === 1 ? '' : 's'}` })),
    };
  }
  // Inside any other string: nothing.
  if ((before.match(/"/g)?.length ?? 0) % 2 === 1) return null;
  // A unit after a number: `3.3V` (only once a letter is typed, or on request).
  const unit = new RegExp(`(?:^|[^\\p{L}\\p{N}_.])(\\d+(?:\\.\\d+)?)(${WORD}*)$`, 'u').exec(before);
  if (unit && (unit[2] !== '' || explicit)) {
    return { from: col - unit[2].length, options: UNITS.map(([label, detail]) => ({ label, kind: 'unit' as const, detail })) };
  }
  // A type after `.name:`.
  const type = new RegExp(`\\.${WORD}+\\s*:\\s*((?:${WORD}|::)*)$`, 'u').exec(before);
  if (type) {
    const typed = type[1];
    return { from: col - (typed.length - (typed.lastIndexOf('::') >= 0 ? typed.lastIndexOf('::') + 2 : 0)), options: typeOptions(data.project) };
  }
  // A member after a dot that starts a path: `.st`, or `.stage.v` after the root.
  const member = new RegExp(`(?:^|[^\\p{L}\\p{N}_)\\]"])((?:\\.${WORD}+)*)\\.(${WORD}*)$`, 'u').exec(before);
  if (member) {
    const path = member[1].split('.').filter(Boolean);
    const options = memberOptions(data, path);
    return options.length || path.length ? { from: col - member[2].length, options } : null;
  }
  // A keyword at the start of a statement.
  const word = new RegExp(`^\\s*([\\p{L}_]${WORD}*)$`, 'u').exec(before);
  if (word) return { from: col - word[1].length, options: [...KEYWORDS, 'pins'].map((k) => ({ label: k, kind: 'keyword' as const })) };
  return null;
}
