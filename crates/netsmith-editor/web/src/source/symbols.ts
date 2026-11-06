// What a source file defines, read from its text: the definitions (`board`, `module`, ...) and,
// inside each, the declarations (`.r1: Resistor { ... }`), nested as the braces nest. The server's
// `Def` list has a line and a name for each definition and nothing else (no end line, no
// declarations), and the editor needs both for the breadcrumbs, the outline and the file-scoped
// go to symbol, and for text that is not saved yet. So the page reads the text itself, with the same
// tokenizer that colours it (`nsm.ts`). Pure: no page, no store.

import { tokenize } from '../nsm';

/** The words that start a definition. */
export const DEFINITION_KINDS = ['board', 'system', 'panel', 'module', 'part', 'interface'] as const;
export type DefinitionKind = (typeof DEFINITION_KINDS)[number];

/** What a declaration stands for, when the kind of its type is known. */
export type Role = 'part' | 'module' | 'port' | 'field';

export interface Sym {
  /** Unique in a file: the line and the name. */
  id: string;
  name: string;
  /** A definition's kind, or `decl` for a declaration. */
  kind: DefinitionKind | 'decl';
  /** The type a declaration names (`Resistor` in `.r1: Resistor`): the last word of a path. */
  typeName?: string;
  /** 1-based first line and last line (the line of the closing brace, or the same line). */
  line: number;
  endLine: number;
  /** 1-based column of the name (after the dot for a declaration). */
  column: number;
  /** 0 for a definition, 1 for what is declared in it, and so on. */
  depth: number;
  children: Sym[];
}

const isDefinitionKind = (w: string): w is DefinitionKind => (DEFINITION_KINDS as readonly string[]).includes(w);

/** The symbols of a text, nested. A definition that is never closed runs to the last line. */
export function scanSymbols(text: string): Sym[] {
  const lines = text.split('\n');
  const roots: Sym[] = [];
  const stack: { sym: Sym; depth: number }[] = [];
  let depth = 0;
  const attach = (s: Sym) => {
    const parent = stack[stack.length - 1]?.sym;
    s.depth = parent ? parent.depth + 1 : 0;
    (parent ? parent.children : roots).push(s);
  };
  lines.forEach((raw, i) => {
    const n = i + 1;
    const toks = tokenize(raw);
    const word = (k: number) => (toks[k] ? raw.slice(toks[k].from, toks[k].to) : '');
    let pending: Sym | null = null;
    // A definition starts at the left margin of the file or at any depth: nothing nests a definition,
    // but a stray one in a body is still told from a declaration by its keyword.
    if (depth === 0 && toks[0]?.kind === 'keyword' && isDefinitionKind(word(0)) && toks[1] && (toks[1].kind === 'type' || toks[1].kind === 'name')) {
      pending = { id: `${n}:${word(1)}`, name: word(1), kind: word(0) as DefinitionKind, line: n, endLine: n, column: toks[1].from + 1, depth: 0, children: [] };
    } else if (depth > 0 && toks[0]?.kind === 'punct' && word(0) === '.' && toks[1]?.kind === 'member' && toks[2]?.kind === 'punct' && word(2) === ':') {
      // `.name: Type` or `.name: path::Type`; a name that carries `.a.b` is a path, not a declaration.
      let k = 3;
      let type = '';
      while (toks[k] && (toks[k].kind === 'type' || toks[k].kind === 'name' || toks[k].kind === 'namespace' || word(k) === '::')) {
        if (word(k) !== '::') type = word(k);
        k++;
      }
      pending = { id: `${n}:${word(1)}`, name: word(1), kind: 'decl', typeName: type || undefined, line: n, endLine: n, column: toks[1].from + 1, depth: 0, children: [] };
    }
    if (pending) attach(pending);
    let bound = false;
    for (let k = 0; k < toks.length; k++) {
      if (toks[k].kind !== 'punct') continue;
      const c = word(k);
      if (c === '{') {
        depth++;
        if (pending && !bound) {
          stack.push({ sym: pending, depth });
          bound = true;
        }
      } else if (c === '}') {
        const top = stack[stack.length - 1];
        if (top && top.depth === depth) {
          top.sym.endLine = n;
          stack.pop();
        }
        depth = Math.max(0, depth - 1);
      }
    }
  });
  // Whatever is still open runs to the end of the file.
  for (const open of stack) open.sym.endLine = lines.length;
  return roots;
}

/** Every symbol, parents before children. */
export function flatten(syms: Sym[]): Sym[] {
  const out: Sym[] = [];
  const walk = (list: Sym[]) => {
    for (const s of list) {
      out.push(s);
      walk(s.children);
    }
  };
  walk(syms);
  return out;
}

/** The chain of symbols that contain a line, outermost first: the definition, then what is declared
 * in it that holds the line, and so on. Empty outside every definition. */
export function containing(syms: Sym[], line: number): Sym[] {
  const chain: Sym[] = [];
  let level = syms;
  for (;;) {
    const hit = level.find((s) => s.line <= line && line <= s.endLine);
    if (!hit) return chain;
    chain.push(hit);
    level = hit.children;
  }
}

/** The symbols beside `sym`: the list it is in, itself included. */
export function siblings(syms: Sym[], sym: Sym): Sym[] {
  const find = (list: Sym[]): Sym[] | null => {
    if (list.includes(sym)) return list;
    for (const s of list) {
      const hit = find(s.children);
      if (hit) return hit;
    }
    return null;
  };
  return find(syms) ?? [sym];
}

/** The doc comment above a line: the run of `///` lines directly before it, without the marks. */
export function docAbove(lines: string[], line: number): string[] {
  const out: string[] = [];
  for (let n = line - 1; n >= 1; n--) {
    const t = lines[n - 1]?.trim() ?? '';
    if (!t.startsWith('///') || t.startsWith('////')) break;
    out.unshift(t.slice(3).replace(/^ /, ''));
  }
  return out;
}

/** What a declaration is, given what is known of the kinds of definitions by name. `kindOf` is the
 * kind of the definition a type names, or undefined if the page does not know it. */
export function roleOf(sym: Sym, kindOf: (type: string) => string | undefined): Role {
  if (sym.kind !== 'decl') return 'field';
  const k = sym.typeName ? kindOf(sym.typeName) : undefined;
  if (k === 'part') return 'part';
  if (k === 'module' || k === 'board' || k === 'system' || k === 'panel') return 'module';
  if (k === 'interface') return 'port';
  return 'field';
}

// The breadcrumbs, the outline and the cards all ask for the symbols of the text on show, on every keystroke:
// the last few texts are scanned once.
const recent: [string, Sym[]][] = [];

/** `scanSymbols`, remembering the last four texts. The result must not be changed. */
export function scanCached(text: string): Sym[] {
  const hit = recent.find(([t]) => t === text);
  if (hit) return hit[1];
  const syms = scanSymbols(text);
  recent.unshift([text, syms]);
  if (recent.length > 4) recent.pop();
  return syms;
}
