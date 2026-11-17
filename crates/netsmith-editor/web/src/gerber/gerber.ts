// The Gerber view of the PCB area (issue 1107): what it says of a fab package, as words and numbers. The package is
// read and drawn by the server with the reader `netsmith view gerber` uses (`GerberView`); the page names its views
// and files by the reader's own words, so a view the reader learns (`--layer`'s) is offered with no change here.

import type { GerberFile, GerberHoles, GerberUnread } from '../api';

/** Screen pixels a millimetre of the drawing at 100 %, as the 2D board's. */
export const SCALE = 12;

/** What the view switch calls a view of the reader (`top`, `inner2`, `bottom`, `all`); a word it does not know is shown as it is. */
export function viewLabel(word: string): string {
  if (word === 'top') return 'Top';
  if (word === 'bottom') return 'Bottom';
  if (word === 'all') return 'All';
  const inner = /^inner(\d+)$/.exec(word);
  if (inner) return `Inner ${inner[1]}`;
  return word;
}

/** What a view shows, for its tip. */
export function viewTip(word: string): string {
  if (word === 'top') return 'The top as a person sees it: copper, mask openings, paste, silkscreen, the outline and the holes';
  if (word === 'bottom') return 'The bottom as a person sees it with the board turned over, mirrored left to right';
  if (word === 'all') return 'Every layer, seen from the top';
  if (/^inner\d+$/.test(word)) return `The inner copper layer ${word.slice(5)}, with the outline and the holes`;
  return `--layer ${word}`;
}

const FUNCTION: Record<string, string> = {
  copper: 'copper',
  mask: 'mask openings',
  paste: 'paste',
  silk: 'silkscreen',
  outline: 'Outline',
  drill: 'Drill',
  other: 'Not known',
};

/** A file's name in the layer list: its layer and function (`Top copper`, `Inner 1 copper`, `Bottom paste`, `Drill`). */
export function fileLabel(f: GerberFile): string {
  const what = FUNCTION[f.function] ?? f.function;
  if (!f.layer) return what.charAt(0).toUpperCase() + what.slice(1);
  return `${viewLabel(f.layer)} ${what}`;
}

/** A point of the SVG (mm, the viewBox's frame) in the viewer's frame: mm from the outline's top left corner, y down.
 * A mirrored drawing (the bottom) has x turned over about the middle of its region. */
export function toFrame(p: { x: number; y: number }, region: number[], mirrored: boolean): { x: number; y: number } {
  const [rx, , rw] = region;
  return mirrored ? { x: 2 * rx + rw - p.x, y: p.y } : { x: p.x, y: p.y };
}

/** The point of the SVG under a pointer at (`px`, `py`) of a box of `width` by `height` screen pixels that shows `region`. */
export function svgPoint(px: number, py: number, width: number, height: number, region: number[]): { x: number; y: number } {
  const [rx, ry, rw, rh] = region;
  return { x: rx + (px / width) * rw, y: ry + (py / height) * rh };
}

/** A measure between two points of the frame, in mm. */
export interface Measure {
  dx: number;
  dy: number;
  d: number;
}

export function measure(a: { x: number; y: number }, b: { x: number; y: number }): Measure {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  return { dx, dy, d: Math.hypot(dx, dy) };
}

const mm = (v: number) => (Math.abs(v) < 0.0005 ? 0 : v).toFixed(3);

/** The ruler's reading: `12.346 mm (Δx 10.000, Δy 7.250)`. */
export function measureText(m: Measure): string {
  return `${mm(m.d)} mm (Δx ${mm(m.dx)}, Δy ${mm(m.dy)})`;
}

/** Where the pointer is, in the frame of the layout and routes files. */
export function pointText(p: { x: number; y: number }): string {
  return `x ${mm(p.x)}  y ${mm(p.y)} mm`;
}

/** One size of hole: `0.300 mm plated: 12 holes, 1 slot`. */
export function holesText(h: GerberHoles): string {
  const parts = [`${h.holes} hole${h.holes === 1 ? '' : 's'}`];
  if (h.slots > 0) parts.push(`${h.slots} slot${h.slots === 1 ? '' : 's'}`);
  return `${h.diameter.toFixed(3)} mm ${h.plated ? 'plated' : 'not plated'}: ${parts.join(', ')}`;
}

/** Where a note of the reader is: `led-F_Cu.gbr, line 12`. */
export function unreadWhere(u: GerberUnread): string {
  return u.line ? `${u.file}, line ${u.line}` : u.file;
}

/** A path as a shell reads it: quoted when it has anything but plain characters. */
function shellWord(s: string): string {
  return /^[\w./@:+-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`;
}

/** The command that draws the same view of the package. */
export function command(path: string, layer: string): string {
  return `netsmith view gerber ${shellWord(path)} --layer ${shellWord(layer)}`;
}
