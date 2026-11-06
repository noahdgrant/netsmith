// The breadcrumbs above the source: the file, the definition the cursor is in, and what is declared
// there at the cursor, each with the list of its siblings for the dropdown. Pure functions of the
// file's symbols (`symbols.ts`) and the project's file list.

import { type Sym, containing, siblings } from './symbols';

export interface Crumb {
  /** `file`, `def` (a board, module, ...) or `decl`. */
  kind: 'file' | 'def' | 'decl';
  label: string;
  /** What the icon shows: a definition's kind, or `file`, or `decl`. */
  icon: string;
  /** The full path of the file, or the symbol. */
  file: string;
  sym?: Sym;
  /** The type of a declaration, muted after its name. */
  detail?: string;
}

export interface CrumbOption {
  id: string;
  label: string;
  detail?: string;
  icon: string;
  /** Where choosing it goes. A file option has no line. */
  file: string;
  line?: number;
  column?: number;
  current: boolean;
}

const baseName = (p: string) => p.split('/').pop() ?? p;
const dirOf = (p: string) => (p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : '');

/** The crumbs for the cursor at `line` of `file`: the file, then the symbols that hold the line. */
export function crumbPath(file: string, syms: Sym[], line: number): Crumb[] {
  const out: Crumb[] = [{ kind: 'file', label: baseName(file), icon: 'file', file }];
  for (const s of containing(syms, line)) {
    out.push({
      kind: s.kind === 'decl' ? 'decl' : 'def',
      label: s.name,
      icon: s.kind,
      file,
      sym: s,
      detail: s.typeName,
    });
  }
  return out;
}

/** What the dropdown of a crumb lists: the files beside the file, or the symbols beside the symbol. */
export function crumbOptions(crumb: Crumb, syms: Sym[], files: string[]): CrumbOption[] {
  if (crumb.kind === 'file') {
    const dir = dirOf(crumb.file);
    return files
      .filter((f) => dirOf(f) === dir)
      .sort((a, b) => baseName(a).localeCompare(baseName(b), undefined, { numeric: true }))
      .map((f) => ({ id: `file:${f}`, label: baseName(f), icon: 'file', file: f, current: f === crumb.file }));
  }
  const sym = crumb.sym;
  if (!sym) return [];
  return siblings(syms, sym).map((s) => ({
    id: `sym:${s.id}`,
    label: s.name,
    detail: s.kind === 'decl' ? s.typeName : s.kind,
    icon: s.kind,
    file: crumb.file,
    line: s.line,
    column: s.column,
    current: s === sym,
  }));
}

/** The index of the crumb whose dropdown `Ctrl+Shift+.` opens first: the innermost. */
export const lastCrumb = (crumbs: Crumb[]): number => crumbs.length - 1;
