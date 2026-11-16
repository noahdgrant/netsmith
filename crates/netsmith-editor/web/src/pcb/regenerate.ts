// Regenerate one module (issue 825): which module the selection is in, and what a proposal says.
// The server places the module again with every other part held and answers with a proposal; the
// page draws it over the board, and Accept sends its changes as one edit (one step of undo).

import type { PcbRegenerated, PcbTotals } from '../api';

/** Whether the part at `path` is in the module at `module` (or is that part), as the server reads it. */
export function inModule(path: string, module: string): boolean {
  if (!module || !path.startsWith(module)) return false;
  const rest = path.slice(module.length);
  return rest === '' || rest.startsWith('.') || rest.startsWith('[');
}

/** The module a part is in: its path less its last word (`a.led` is in `a`, `bank[2]` in `bank`); null for a part of the board itself. */
export function moduleOf(path: string): string | null {
  const cut = Math.max(path.lastIndexOf('.'), path.lastIndexOf('['));
  return cut > 0 ? path.slice(0, cut) : null;
}

/** The innermost module that holds every one of `paths`; null when they share none. */
export function moduleOfAll(paths: string[]): string | null {
  if (paths.length === 0) return null;
  for (let m = moduleOf(paths[0]); m; m = moduleOf(m)) {
    const module = m;
    if (paths.every((p) => inModule(p, module))) return module;
  }
  return null;
}

/** One row of the tree of a board's modules: a module instance, as deep as it is nested, with the parts in it. */
export interface ModuleRow {
  /** Its path, as the server reads it: `a`, `stage.bank`, `stage.bank[2]`. */
  path: string;
  /** What the row shows: its path less that of the module it is in (`bank`, `[2]`). */
  label: string;
  /** How many modules hold it. */
  depth: number;
  /** The parts in it, at any depth. */
  parts: number;
}

/** The modules of a board, from the paths of its parts, as the rows of a tree in order: every prefix of a
 * path that ends before a `.` or a `[` is a module, as the server's `module_paths` reads them. */
export function moduleRows(paths: string[]): ModuleRow[] {
  const count = new Map<string, number>();
  for (const p of paths) {
    for (let m = moduleOf(p); m; m = moduleOf(m)) count.set(m, (count.get(m) ?? 0) + 1);
  }
  const parentOf = (m: string): string | null => {
    for (let up = moduleOf(m); up; up = moduleOf(up)) if (count.has(up)) return up;
    return null;
  };
  const children = new Map<string | null, string[]>();
  for (const m of count.keys()) {
    const up = parentOf(m);
    children.set(up, [...(children.get(up) ?? []), m]);
  }
  const rows: ModuleRow[] = [];
  const walk = (up: string | null, depth: number) => {
    for (const m of (children.get(up) ?? []).sort((x, y) => x.localeCompare(y, undefined, { numeric: true }))) {
      const label = up ? m.slice(up.length).replace(/^\./, '') : m;
      rows.push({ path: m, label, depth, parts: count.get(m)! });
      walk(m, depth + 1);
    }
  };
  walk(null, 0);
  return rows;
}

const mm = (v: number) => `${Math.round(v * 100) / 100}`;

/** One line of a total before and after, with the change: `hpwl 36.63 → 33.39 mm (−3.24)`. */
export function totalLine(name: string, before: number, after: number, unit: string): string {
  const d = Math.round((after - before) * 100) / 100;
  const change = d === 0 ? 'no change' : `${d > 0 ? '+' : '−'}${mm(Math.abs(d))}`;
  return `${name} ${mm(before)} → ${mm(after)}${unit} (${change})`;
}

/** The lines of the totals of a proposal. */
export function totalsLines(before: PcbTotals, after: PcbTotals): string[] {
  return [
    totalLine('wire (hpwl)', before.hpwl, after.hpwl, ' mm'),
    totalLine('ratsnest', before.ratsnest, after.ratsnest, ' mm'),
    totalLine('crossings', before.crossings, after.crossings, ''),
  ];
}

/** What the page says when a proposal is shown. */
export function proposalSaid(p: PcbRegenerated): string {
  const parts = `${p.parts.length} part${p.parts.length === 1 ? '' : 's'}`;
  const held = p.held.length > 0 ? `, ${p.held.length} held` : '';
  return `Regenerate ${p.module} (seed ${p.seed}): ${parts} placed again${held}; Accept to keep it`;
}
