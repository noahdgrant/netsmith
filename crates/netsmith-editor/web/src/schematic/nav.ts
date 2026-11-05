// Moving about a schematic of many sheets: the order of the sheets, which sheets carry a net,
// the order a net's pins are walked in, and what to say about a sheet. Pure functions.

import type { Location, NetInfo, PartInfo, PinInfo, Problem } from '../api';
import { covers } from '../util';
import type { DrawingIndex, Sheet } from './drawing';
import type { Box } from './zoom';

/** `adc: AdcMonitor` is the instance `adc` of the module `AdcMonitor`. */
export function parseTitle(title: string): { name: string; detail: string } {
  const at = title.indexOf(': ');
  return at < 0 ? { name: title, detail: '' } : { name: title.slice(0, at), detail: title.slice(at + 2) };
}

/** `stage (2/3)` is the second of the three sheets `stage` was cut into. */
export function pieceOf(name: string): { base: string; piece: number } {
  const m = /^(.*) \((\d+)\/\d+\)$/.exec(name);
  return m ? { base: m[1], piece: Number(m[2]) } : { base: name, piece: 0 };
}

type Placed = Pick<PartInfo, 'refdes' | 'decl' | 'scopes'>;

/** Whether `a` is earlier in the source than `b`: in the board's file first, then in the other
 * files by name, then by line and column. */
function before(a: Location, b: Location, boardFile: string | null): boolean {
  if (a.file !== b.file) {
    if (a.file === boardFile) return true;
    if (b.file === boardFile) return false;
    return a.file < b.file;
  }
  return a.line !== b.line ? a.line < b.line : a.column < b.column;
}

/** Where in the source a sheet's module instance is declared: the outermost declaration that
 * contains one of its parts (`.adc: AdcMonitor`), or the part's own when it is in no module. */
function declaredAt(parts: Placed[], boardFile: string | null): Location | null {
  let best: Location | null = null;
  for (const p of parts) {
    const at = p.scopes.length > 0 ? p.scopes[p.scopes.length - 1] : p.decl;
    if (at && (!best || before(at, best, boardFile))) best = at;
  }
  return best;
}

/** The sheets in the order their modules are declared in the source, not alphabetical. A sheet
 * cut in pieces keeps its pieces together in order; a sheet with no part that says where it is
 * from goes last, in the order it came in. */
export function orderSheets(
  sheets: Sheet[],
  partSheet: Map<string, string>,
  parts: Placed[],
  boardFile: string | null = null,
): Sheet[] {
  const byBase = new Map<string, Placed[]>();
  for (const p of parts) {
    const sheet = partSheet.get(p.refdes);
    if (sheet == null) continue;
    const base = pieceOf(sheet).base;
    if (!byBase.has(base)) byBase.set(base, []);
    byBase.get(base)!.push(p);
  }
  const where = new Map<string, Location | null>();
  for (const s of sheets) {
    const base = pieceOf(s.name).base;
    if (!where.has(base)) where.set(base, declaredAt(byBase.get(base) ?? [], boardFile));
  }
  const rank = new Map(sheets.map((s, i) => [s.name, i]));
  return [...sheets].sort((a, b) => {
    const pa = pieceOf(a.name);
    const pb = pieceOf(b.name);
    const la = where.get(pa.base) ?? null;
    const lb = where.get(pb.base) ?? null;
    if (la && lb) {
      if (pa.base !== pb.base) {
        if (before(la, lb, boardFile)) return -1;
        if (before(lb, la, boardFile)) return 1;
      } else if (pa.piece !== pb.piece) return pa.piece - pb.piece;
    } else if (la) return -1;
    else if (lb) return 1;
    return rank.get(a.name)! - rank.get(b.name)!;
  });
}

const natural = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });

/** The pins of a net on each sheet, by the sheet of their part. A pin whose part is not drawn is
 * left out. */
export function netSheets(pins: PinInfo[], partSheet: Map<string, string>): Map<string, PinInfo[]> {
  const out = new Map<string, PinInfo[]>();
  for (const p of pins) {
    const sheet = partSheet.get(p.refdes);
    if (sheet == null) continue;
    if (!out.has(sheet)) out.set(sheet, []);
    out.get(sheet)!.push(p);
  }
  return out;
}

/** A sheet a net can be followed to. */
export interface Target {
  sheet: string;
  /** The pins it has there. */
  pins: PinInfo[];
}

/** The sheets other than `here` that carry the net, in the order of `order` (the sheet names in
 * the order of the strip): what a click on a flag offers. */
export function otherSheets(net: NetInfo | undefined, here: string, partSheet: Map<string, string>, order: string[]): Target[] {
  if (!net) return [];
  const by = netSheets(net.pins, partSheet);
  return order.filter((s) => s !== here && by.has(s)).map((s) => ({ sheet: s, pins: by.get(s)! }));
}

/** `N pins on M modules`. */
export function netSummary(net: NetInfo, partSheet: Map<string, string>): { pins: number; sheets: number; text: string } {
  const sheets = netSheets(net.pins, partSheet).size;
  const pins = net.pins.length;
  const text = `${pins} ${pins === 1 ? 'pin' : 'pins'} on ${sheets} ${sheets === 1 ? 'module' : 'modules'}`;
  return { pins, sheets, text };
}

/** The pins of a net in the order Tab walks them: sheet by sheet in the order of the strip, then
 * by designator and pin number as people count (C2 before C10). */
export function netNavOrder(pins: PinInfo[], partSheet: Map<string, string>, order: string[]): PinInfo[] {
  const rank = new Map(order.map((s, i) => [s, i]));
  const at = (p: PinInfo) => rank.get(partSheet.get(p.refdes) ?? '') ?? order.length;
  return [...pins].sort((a, b) => at(a) - at(b) || natural(a.refdes, b.refdes) || natural(a.pin, b.pin));
}

/** The index after `at` (or before, with `dir` of -1) in a ring of `n`; the first or last when
 * there is no current one. */
export function stepRing(n: number, at: number, dir: 1 | -1): number {
  if (n <= 0) return -1;
  if (at < 0) return dir === 1 ? 0 : n - 1;
  return (at + dir + n) % n;
}

/** What a sheet holds. */
export interface SheetStats {
  parts: number;
  /** Nets drawn on the sheet. */
  nets: number;
  /** Nets that leave it through a flag. */
  leaving: number;
  errors: number;
  warnings: number;
  notes: number;
}

/** The counts for each sheet, from the drawing and the problems; a problem counts for the sheet
 * of a part whose declaration (or one that contains it) the problem is at. */
export function sheetStats(
  sheets: Sheet[],
  index: DrawingIndex,
  parts: Placed[],
  problems: Pick<Problem, 'severity' | 'at'>[],
): Map<string, SheetStats> {
  const out = new Map<string, SheetStats>();
  for (const s of sheets) out.set(s.name, { parts: 0, nets: 0, leaving: 0, errors: 0, warnings: 0, notes: 0 });
  for (const [, sheet] of index.partSheet) {
    const s = out.get(sheet);
    if (s) s.parts++;
  }
  for (const [, places] of index.netPlaces) {
    for (const [sheet, place] of places) {
      const s = out.get(sheet);
      if (!s) continue;
      s.nets++;
      if (place.flags.length > 0) s.leaving++;
    }
  }
  for (const pr of problems) {
    if (!pr.at) continue;
    const at = pr.at;
    // The innermost declaration that covers the line decides.
    let sheet: string | undefined;
    let width = Infinity;
    for (const p of parts) {
      for (const d of [p.decl, ...p.scopes]) {
        if (d && covers(d, at.file, at.line) && d.endLine - d.line < width) {
          width = d.endLine - d.line;
          sheet = index.partSheet.get(p.refdes);
        }
      }
    }
    const s = sheet != null ? out.get(sheet) : undefined;
    if (!s) continue;
    if (pr.severity === 'error') s.errors++;
    else if (pr.severity === 'warning') s.warnings++;
    else s.notes++;
  }
  return out;
}

/** The sheets whose name, module or one of whose parts' designators contain `query`. */
export function filterSheets(sheets: Sheet[], query: string, partsOf: (sheet: string) => string[]): Sheet[] {
  const q = query.trim().toLowerCase();
  if (!q) return sheets;
  return sheets.filter((s) => s.title.toLowerCase().includes(q) || partsOf(s.name).some((r) => r.toLowerCase() === q));
}

/** The box of a part and of the parts it is wired to on its own sheet: what to show round a part so
 * it is seen in its circuit. A net of more than `maxPins` pins (ground, a supply) is left out, or
 * every part would be a neighbour. */
export function neighbourBox(
  ref: string,
  nets: Pick<NetInfo, 'pins'>[],
  partSheet: Map<string, string>,
  partBox: Map<string, Box>,
  maxPins = 6,
): Box | undefined {
  const own = partBox.get(ref);
  if (!own) return undefined;
  const sheet = partSheet.get(ref);
  const near = new Set<string>();
  for (const n of nets) {
    if (n.pins.length > maxPins || !n.pins.some((p) => p.refdes === ref)) continue;
    for (const p of n.pins) if (p.refdes !== ref && partSheet.get(p.refdes) === sheet) near.add(p.refdes);
  }
  let x0 = own.x;
  let y0 = own.y;
  let x1 = own.x + own.w;
  let y1 = own.y + own.h;
  for (const r of near) {
    const b = partBox.get(r);
    if (!b) continue;
    x0 = Math.min(x0, b.x);
    y0 = Math.min(y0, b.y);
    x1 = Math.max(x1, b.x + b.w);
    y1 = Math.max(y1, b.y + b.h);
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** What to do with a reveal that was asked for: show it (`go`), keep it until the drawing is the
 * board's (`wait`), or forget it (`drop`). A request is for the board that was in front when it
 * was made; it is shown only on a drawing, and with a board detail, of that same board, never on
 * the previous board's drawing that is still on screen while the new one is on its way. */
export type RevealStep = 'go' | 'wait' | 'drop';
export function revealStep(
  request: { board: string | null },
  /** The board in front now. */
  current: string | null,
  /** The board the drawing in the page was made for (null: none yet). */
  drawingBoard: string | null,
  /** The board the detail in the page is for. */
  detailBoard: string | null,
): RevealStep {
  if (request.board !== current) return 'drop';
  if (current === null || drawingBoard !== current || detailBoard !== current) return 'wait';
  return 'go';
}

/** The module instance (`adc`) that a sheet of the overview stands for, when the sheet is one
 * (the parts that are in no module sit on a sheet named for the board, which is no module). A
 * sheet cut in pieces (`stage (2/3)`) is its module's. */
export function moduleOfSheet(sheet: string, modules: string[]): string | null {
  const base = pieceOf(sheet).base;
  return modules.includes(base) ? base : null;
}
