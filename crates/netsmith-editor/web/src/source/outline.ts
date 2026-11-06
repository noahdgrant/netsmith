// The outline of the open file: its definitions and what each declares, as rows of a tree with a
// filter, problem badges and the entry the cursor is in. Pure: the panel draws the rows.

import { type Role, type Sym, containing, roleOf } from './symbols';

export interface ProblemAt {
  line: number;
  endLine: number;
  severity: 'error' | 'warning' | 'note';
}

export interface OutlineRow {
  sym: Sym;
  /** 0 for a definition. */
  level: number;
  role: Role | 'definition';
  /** True or false for a row with children, null for one without. */
  open: boolean | null;
  errors: number;
  warnings: number;
  /** The filter's match in the name, as [from, to), or null. */
  mark: [number, number] | null;
}

export interface OutlineOptions {
  kindOf: (type: string) => string | undefined;
  filter: string;
  /** Ids of the symbols the person folded. */
  folded: ReadonlySet<string>;
  problems: ProblemAt[];
}

const matches = (sym: Sym, needle: string): [number, number] | null => {
  const at = sym.name.toLowerCase().indexOf(needle);
  return at < 0 ? null : [at, at + needle.length];
};

const typeMatches = (sym: Sym, needle: string) => !!sym.typeName && sym.typeName.toLowerCase().includes(needle);

/** The rows that are on show. With a filter, a symbol shows if its name or type matches or something
 * inside it does; the folds are then ignored so every match is seen. */
export function outlineRows(syms: Sym[], opts: OutlineOptions): OutlineRow[] {
  const needle = opts.filter.trim().toLowerCase();
  const rows: OutlineRow[] = [];
  const count = (s: Sym, severity: 'error' | 'warning') => opts.problems.filter((p) => p.severity === severity && p.line >= s.line && p.line <= s.endLine).length;
  // Whether something at or below `s` matches the filter.
  const any = (s: Sym): boolean => !!matches(s, needle) || typeMatches(s, needle) || s.children.some(any);
  const walk = (list: Sym[], level: number) => {
    for (const s of list) {
      if (needle && !any(s)) continue;
      const kids = needle ? s.children.filter(any) : s.children;
      const open = kids.length === 0 ? null : needle ? true : !opts.folded.has(s.id);
      rows.push({
        sym: s,
        level,
        role: s.kind === 'decl' ? roleOf(s, opts.kindOf) : 'definition',
        open,
        errors: count(s, 'error'),
        warnings: count(s, 'warning'),
        mark: needle ? matches(s, needle) : null,
      });
      if (open) walk(kids, level + 1);
    }
  };
  walk(syms, 0);
  return rows;
}

/** The id of the entry the cursor is in: the innermost symbol that holds the line. */
export function activeEntry(syms: Sym[], line: number | null): string | null {
  if (line == null) return null;
  const chain = containing(syms, line);
  return chain.length ? chain[chain.length - 1].id : null;
}

/** The ids of the symbols that hold the entry, so a folded parent can be opened for it. */
export function ancestorsOf(syms: Sym[], id: string): string[] {
  const find = (list: Sym[], trail: string[]): string[] | null => {
    for (const s of list) {
      if (s.id === id) return trail;
      const hit = find(s.children, [...trail, s.id]);
      if (hit) return hit;
    }
    return null;
  };
  return find(syms, []) ?? [];
}
