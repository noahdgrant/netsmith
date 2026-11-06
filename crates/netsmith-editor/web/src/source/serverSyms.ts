// The symbols of a file as the server reports them: `Def.endLine` and `Def.children` (the declarations written directly in a
// definition's body) say where each definition ends and what is in it, so the outline and the breadcrumbs need not read the text.
// The page's own reader (`symbols.ts`) stays for what the server cannot answer: a server that predates the fields, and text that
// is not what is on disk. The server's answer is about the saved file, so it is used only when it still fits the text on the page
// (each name stands on its line, each definition ends on a closing brace); when it does not, the caller reads the text.
// Pure: no page, no store.

import type { Def, DefChild } from '../api';
import { DEFINITION_KINDS, type DefinitionKind, type Sym } from './symbols';

const isDefinitionKind = (k: string): k is DefinitionKind => (DEFINITION_KINDS as readonly string[]).includes(k);

/** The column of `name` in `line` (1-based), after the dot for a declaration, or 0 when the line does not hold it. */
function columnOf(line: string | undefined, name: string, decl: boolean): number {
  if (line === undefined) return 0;
  const at = line.indexOf(decl ? `.${name.split('.')[0]}` : name);
  return at < 0 ? 0 : at + 1 + (decl ? 1 : 0);
}

/** The symbols of the server's definitions, when every one of them stands where the text says; null otherwise (no definitions, a server
 * that does not send the end of a definition, or a file that changed since the server read it). */
export function symsFromDefs(defs: readonly Def[] | undefined, text: string): Sym[] | null {
  if (!defs || defs.length === 0) return null;
  const lines = text.split('\n');
  const out: Sym[] = [];
  for (const def of defs) {
    if (def.endLine === undefined || !isDefinitionKind(def.kind)) return null;
    const head = lines[def.line - 1];
    const column = columnOf(head, def.name, false);
    if (!column || def.endLine < def.line || def.endLine > lines.length) return null;
    // The end of a definition with a body is its closing brace.
    if (def.endLine > def.line && !lines[def.endLine - 1].includes('}')) return null;
    const sym: Sym = { id: `${def.line}:${def.name}`, name: def.name, kind: def.kind, line: def.line, endLine: def.endLine, column, depth: 0, children: [] };
    for (const child of def.children ?? []) {
      const c = childSym(child, lines, def);
      if (!c) return null;
      sym.children.push(c);
    }
    out.push(sym);
  }
  return out;
}

function childSym(child: DefChild, lines: string[], parent: Def): Sym | null {
  const column = columnOf(lines[child.line - 1], child.name, true);
  if (!column || child.line < parent.line || child.endLine < child.line || child.endLine > (parent.endLine ?? 0)) return null;
  return { id: `${child.line}:${child.name}`, name: child.name, kind: 'decl', typeName: child.typeName || undefined, line: child.line, endLine: child.endLine, column, depth: 1, children: [] };
}

/** The symbols of a file for the outline and the breadcrumbs: the server's when it has them and they fit the text, else the text read by
 * the page (`scan`). */
export function fileSymbols(defs: readonly Def[] | undefined, text: string, scan: (text: string) => Sym[]): { syms: Sym[]; from: 'server' | 'text' } {
  const server = symsFromDefs(defs, text);
  return server ? { syms: server, from: 'server' } : { syms: scan(text), from: 'text' };
}
