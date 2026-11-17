// The modules lit on the board (issue 1129): all the parts of a module instance shown at once in the 2D and 3D PCB
// views, each module in its own colour, so where a module's parts are and how far they spread shows at a glance. A
// module is lit from the PCB panel's Modules tab (a click; ctrl or shift adds one, a click on one lit takes it off),
// from a part's inspector, from the page's selection (a module block picked in the schematic, go to anything), and
// by `/api/view` (`netsmith view editor-3d --highlight module:<path>`). Hovering a module lights it while the
// pointer is there, in the colour it would take.
//
// A module is every prefix of a part's path that ends before a `.` or a `[` (regenerate.ts, `inModule`), as the
// server's `module_paths` reads it. The colours are the theme's `--module-1` to `--module-6` (themes.css), the same
// as `netsmith view 3d` and `view pcb` draw (netsmith_pcb::MODULE_COLOURS_LIGHT and _DARK). Nothing here touches
// the page: the views read `litModules` and `tintsOf`.

import { signal } from '@preact/signals';

import { inModule, moduleOf } from './regenerate';

/** How many colours the modules take before the first is used again. */
export const MODULE_SLOTS = 6;

/** A module lit: its path and the colour it is drawn in (0 to MODULE_SLOTS - 1). */
export interface LitModule {
  path: string;
  slot: number;
}

/** The modules lit, of one board (a path is one board's): another board shows none. */
export const litModules = signal<{ board: string | null; list: LitModule[] }>({ board: null, list: [] });

/** The modules lit on `board`. */
export function litOn(board: string | null, lit = litModules.value): LitModule[] {
  return board !== null && lit.board === board ? lit.list : [];
}

/** Light `list` on `board` (it replaces what was lit). */
export function setLit(board: string, list: LitModule[]) {
  litModules.value = { board, list };
}

/** The theme token of a colour. */
export const moduleToken = (slot: number): string => `--module-${(slot % MODULE_SLOTS) + 1}`;

/** The colour the next module lit takes: the first one no module lit has, else the next round. */
export function nextSlot(list: LitModule[]): number {
  for (let s = 0; s < MODULE_SLOTS; s++) if (!list.some((m) => m.slot === s)) return s;
  return list.length % MODULE_SLOTS;
}

/** A click on the module `path`: one lit already goes off; else with `add` it joins the others, and without it it is
 * the only one lit. */
export function toggleModule(list: LitModule[], path: string, add: boolean): LitModule[] {
  if (list.some((m) => m.path === path)) return list.filter((m) => m.path !== path);
  if (add) return [...list, { path, slot: nextSlot(list) }];
  return [{ path, slot: 0 }];
}

/** The modules `paths` lit in that order, each in the next colour, as `--highlight` names them. */
export const inOrder = (paths: string[]): LitModule[] => [...new Set(paths)].map((path, i) => ({ path, slot: i % MODULE_SLOTS }));

/** The modules of the parts at `paths`, sorted. */
export function modulePaths(paths: string[]): string[] {
  const out = new Set<string>();
  for (const p of paths) for (let m = moduleOf(p); m; m = moduleOf(m)) out.add(m);
  return [...out].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
}

/** The modules that hold the part at `path`, innermost first (`a.b.c` is in `a.b`, then `a`). */
export function modulesAround(path: string): string[] {
  const out: string[] = [];
  for (let m = moduleOf(path); m; m = moduleOf(m)) out.push(m);
  return out;
}

/** The colour slot of each part lit, by path: the innermost module lit that holds it, and a module the pointer is on
 * (`hovered`) over all of them, in its own colour if it is lit and in the one it would take if not. */
export function tintsOf(parts: string[], list: LitModule[], hovered: string | null): Map<string, number> {
  const out = new Map<string, number>();
  for (const p of parts) {
    let best: LitModule | null = null;
    for (const m of list) if (inModule(p, m.path) && (!best || m.path.length > best.path.length)) best = m;
    if (best) out.set(p, best.slot);
  }
  if (hovered) {
    const slot = list.find((m) => m.path === hovered)?.slot ?? nextSlot(list);
    for (const p of parts) if (inModule(p, hovered)) out.set(p, slot);
  }
  return out;
}

/** The modules lit or hovered, each with its slot and parts, in the order lit (a hovered one last): what the views
 * outline as a whole. */
export function groupsOf(parts: string[], list: LitModule[], hovered: string | null): { path: string; slot: number; parts: string[]; hovered: boolean }[] {
  const groups = list.map((m) => ({ path: m.path, slot: m.slot, parts: parts.filter((p) => inModule(p, m.path)), hovered: m.path === hovered }));
  if (hovered && !list.some((m) => m.path === hovered)) groups.push({ path: hovered, slot: nextSlot(list), parts: parts.filter((p) => inModule(p, hovered)), hovered: true });
  return groups.filter((g) => g.parts.length > 0);
}
