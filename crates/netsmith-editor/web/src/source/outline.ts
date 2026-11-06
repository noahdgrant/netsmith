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

// ---- which definitions are folded ----
// By default the outline shows what the cursor is in and the names of the rest: a definition that does not hold the cursor's line
// is folded, with a count of what it declares. The person's own choices (a twisty, Expand all, Collapse all) are kept for the
// file by the names of the entries (a line number changes with every edit), and win over the default.

/** What the person chose in one file: the keys of the entries they opened and of those they folded. */
export interface FoldChoice {
  open: string[];
  closed: string[];
}

export const NO_CHOICE: FoldChoice = { open: [], closed: [] };

/** An entry's key in a file: the kind and name of a definition, then the names of the declarations inside it. */
export function foldKeys(syms: Sym[]): Map<string, string> {
  const keys = new Map<string, string>();
  const walk = (list: Sym[], parent: string) => {
    for (const s of list) {
      const key = parent ? `${parent}/${s.name}` : `${s.kind}:${s.name}`;
      keys.set(s.id, key);
      walk(s.children, key);
    }
  };
  walk(syms, '');
  return keys;
}

/** The ids of the entries that are folded: the person's choices, and for the rest the definitions (top level) that do not hold `line`. */
export function foldedIds(syms: Sym[], line: number | null, choice: FoldChoice = NO_CHOICE): Set<string> {
  const keys = foldKeys(syms);
  const open = new Set(choice.open);
  const closed = new Set(choice.closed);
  const folded = new Set<string>();
  for (const s of syms) {
    if (s.children.length === 0) continue;
    const holds = line != null && line >= s.line && line <= s.endLine;
    const key = keys.get(s.id)!;
    if (closed.has(key) || (!open.has(key) && !holds)) folded.add(s.id);
  }
  // A declaration the person folded stays folded (nothing folds a declaration by itself).
  const walk = (list: Sym[]) => {
    for (const s of list) {
      if (s.children.length > 0 && closed.has(keys.get(s.id)!)) folded.add(s.id);
      walk(s.children);
    }
  };
  walk(syms.flatMap((s) => s.children));
  return folded;
}

/** The choice after the twisty of an entry was pressed: it was `folded`, so it opens (and the other way). */
export function toggled(choice: FoldChoice, key: string, wasFolded: boolean): FoldChoice {
  const open = choice.open.filter((k) => k !== key);
  const closed = choice.closed.filter((k) => k !== key);
  (wasFolded ? open : closed).push(key);
  return { open, closed };
}

/** Every entry that has children opened, or every one folded: the pair of buttons at the head of the outline. */
export function allChoice(syms: Sym[], fold: boolean): FoldChoice {
  const keys = foldKeys(syms);
  const withKids: string[] = [];
  const walk = (list: Sym[]) => {
    for (const s of list) {
      if (s.children.length > 0) withKids.push(keys.get(s.id)!);
      walk(s.children);
    }
  };
  walk(syms);
  return fold ? { open: [], closed: withKids } : { open: withKids, closed: [] };
}

/** How many declarations an entry holds, all levels: the count on a folded row. */
export function holds(s: Sym): number {
  return s.children.reduce((n, c) => n + 1 + holds(c), 0);
}
