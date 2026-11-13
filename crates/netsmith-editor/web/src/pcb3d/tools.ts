// The 3D view's tools as plain numbers and words: the measure tool (what a point snaps to, the
// distance and its parts, a part's height and its margin to the limit above it), the section plane
// (which side is kept, as `netsmith render --section` keeps it), the exploded stackup (where each
// layer goes) and the `netsmith render` command that draws the view as the editor shows it.
// Everything is in the scene's frame of docs/BOARD3D.md: millimetres, z up, the origin at the
// outline's bottom-left corner on the board's top face. Nothing here touches three.js, and nothing
// here is a check: the numbers are the file's, and the view decides nothing of its own.

import type { CaseMode } from './inspectMode';
import type { Face, LimitInfo, PartInfo3d, StackLayer } from './model';

/** A point or a direction in the scene's frame (mm, z up). */
export type P3 = [number, number, number];

// ---- lengths ----

/** A length for the tools: two or three decimals (`1.60`, `0.035`, `12.346`), no negative zero. */
export function fixed(v: number): string {
  const r = Math.abs(v) < 0.0005 ? 0 : v;
  const t = r.toFixed(3);
  return t.endsWith('0') ? t.slice(0, -1) : t;
}

/** A length with its unit. */
export const mm3 = (v: number): string => `${fixed(v)} mm`;

// ---- the measure tool ----

/** What a point of a measurement is on, in the order the tool prefers them. */
export type SnapKind = 'pad' | 'corner' | 'edge' | 'top' | 'face' | 'surface';

export interface Snap {
  kind: SnapKind;
  /** Where, in the scene's frame. */
  at: P3;
  /** What it is, for a person: `Pad J1.2`, `Top of U1`, `Board edge`. */
  label: string;
}

/** How near the pointer (in pixels of the window) a target must be to be snapped to; a face or a
 * surface is where the pointer is. */
export const SNAP_PX: Record<SnapKind, number> = { pad: 12, corner: 9, edge: 9, top: 0, face: 0, surface: 0 };

const ORDER: SnapKind[] = ['pad', 'corner', 'edge', 'top', 'face', 'surface'];

/** The target a pointer snaps to among those found near it, each with its distance from the
 * pointer in pixels: the first kind of `ORDER` in its reach, and of that kind the nearest. */
export function bestSnap(found: { snap: Snap; px: number }[]): Snap | null {
  for (const kind of ORDER) {
    let best: { snap: Snap; px: number } | null = null;
    for (const f of found) if (f.snap.kind === kind && f.px <= SNAP_PX[kind] && (!best || f.px < best.px)) best = f;
    if (best) return best.snap;
  }
  return null;
}

export interface Measurement {
  id: number;
  a: Snap;
  b: Snap;
}

/** The distance from `a` to `b` and its parts along x, y and z (from `a` to `b`). */
export function delta(a: P3, b: P3): { d: number; dx: number; dy: number; dz: number } {
  const [dx, dy, dz] = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
  return { d: Math.hypot(dx, dy, dz), dx, dy, dz };
}

/** What a measurement's label says: the distance, then dx, dy and dz. */
export function measureWords(a: P3, b: P3): { distance: string; parts: string } {
  const m = delta(a, b);
  return { distance: mm3(m.d), parts: `dx ${fixed(m.dx)} · dy ${fixed(m.dy)} · dz ${fixed(m.dz)}` };
}

/** A height limit's volume in plan, as the view finds it from the volume's mesh. */
export interface LimitPlan {
  info: LimitInfo;
  min: [number, number];
  max: [number, number];
}

/** The face a part stands on: z = 0 for the top, z = -thickness for the bottom. */
export const faceZ = (side: Face, thickness: number): number => (side === 'top' ? 0 : -thickness);

/** What the tool says of one picked part: its height above the face it stands on, and its margin
 * to the height limit above it (the least, where several are), from the file's numbers. An unknown
 * height has neither: it is not measured and not guessed (ADR 0094). */
export function partReadout(
  part: PartInfo3d,
  plan: { min: [number, number]; max: [number, number] } | null,
  limits: LimitPlan[],
): { height: string; limit: string; marginMm: number | null; limitName: string | null; limitMm: number | null } {
  const over = plan
    ? limits.filter((l) => l.info.face === part.side && l.min[0] < plan.max[0] && l.max[0] > plan.min[0] && l.min[1] < plan.max[1] && l.max[1] > plan.min[1])
    : [];
  if (part.heightMm === null || part.provenance === 'unknown') {
    return {
      height: `Height not known: not measured, not guessed`,
      limit: over.length ? `Under the limit ${over[0].info.name} (${mm3(over[0].info.maxHeightMm)}), not checked` : 'No height limit above it',
      marginMm: null,
      limitName: over[0]?.info.name ?? null,
      limitMm: over[0]?.info.maxHeightMm ?? null,
    };
  }
  const h = part.heightMm;
  const height = `${mm3(h)} above the ${part.side} face`;
  if (!over.length) return { height, limit: 'No height limit above it', marginMm: null, limitName: null, limitMm: null };
  const tight = over.reduce((a, b) => (b.info.maxHeightMm - h < a.info.maxHeightMm - h ? b : a));
  const margin = tight.info.maxHeightMm - h;
  const limit =
    margin < 0 ? `${mm3(-margin)} over the limit ${tight.info.name} (${mm3(tight.info.maxHeightMm)})` : `${mm3(margin)} to spare under the limit ${tight.info.name} (${mm3(tight.info.maxHeightMm)})`;
  return { height, limit, marginMm: margin, limitName: tight.info.name, limitMm: tight.info.maxHeightMm };
}

// ---- the section plane ----

export type Axis = 'x' | 'y' | 'z';

export const AXES: Axis[] = ['x', 'y', 'z'];

const AXIS_INDEX: Record<Axis, number> = { x: 0, y: 1, z: 2 };

/** Whether the cut keeps what is below its position along the axis, seen from `towardEye` (the
 * direction from the target to the camera): the side nearer the camera goes, so the cut is seen,
 * and a view square to the axis keeps the lower side. As `netsmith render --section` decides. */
export function keepBelow(towardEye: P3, axis: Axis): boolean {
  return towardEye[AXIS_INDEX[axis]] >= 0;
}

/** Where a cut may be along an axis: the scene's box, in mm. */
export function axisRange(axis: Axis, box: { min: P3; max: P3 }): [number, number] {
  const i = AXIS_INDEX[axis];
  return [box.min[i], box.max[i]];
}

/** Where a cut starts along an axis: the middle of the board in plan, the middle of its thickness in z. */
export function defaultCut(axis: Axis, board: { widthMm: number; heightMm: number; thicknessMm: number }): number {
  return axis === 'x' ? board.widthMm / 2 : axis === 'y' ? board.heightMm / 2 : -board.thicknessMm / 2;
}

/** A position typed by a person, in mm, held to the range; null when it is not a number. */
export function parseCut(text: string, range: [number, number]): number | null {
  const v = Number(text.trim().replace(/\s*mm$/i, '').replace(',', '.'));
  if (!text.trim() || !Number.isFinite(v)) return null;
  return Math.min(range[1], Math.max(range[0], v));
}

// ---- the stackup pulled apart ----

/** One layer of the exploded stackup: a layer of the file's stackup, or a face's silkscreen. */
export interface Slot {
  /** The layer's name (`F.Cu`), or `silk:top` / `silk:bottom`. */
  key: string;
  label: string;
  /** Its kind and thickness, in words. */
  detail: string;
  zTop: number;
  zBottom: number;
  /** How far it moves up (mm) when the stackup is pulled fully apart. */
  offset: number;
}

const KIND_WORDS: Record<StackLayer['kind'], string> = { mask: 'solder mask', copper: 'copper', core: 'core', prepreg: 'prepreg' };

/** The gap between two layers pulled apart: a share of the board's size, so it reads at any size. */
export function explodeGap(widthMm: number, heightMm: number): number {
  return Math.min(12, Math.max(3, 0.1 * Math.max(widthMm, heightMm)));
}

/** The layers in order from the top (the top face's silkscreen when the file has it, the stackup
 * top face down, then the bottom's silkscreen), each moved so the stack spreads about its middle. */
export function explodeSlots(layers: StackLayer[], silk: { top: boolean; bottom: boolean }, gap: number): Slot[] {
  const out: Omit<Slot, 'offset'>[] = [];
  const top = layers[0]?.zTopMm ?? 0;
  const bottom = layers[layers.length - 1]?.zBottomMm ?? 0;
  if (silk.top) out.push({ key: 'silk:top', label: 'Silkscreen (top)', detail: 'ink on the mask', zTop: top, zBottom: top });
  for (const l of layers) out.push({ key: l.name, label: l.name, detail: `${KIND_WORDS[l.kind]} · ${mm3(l.thicknessMm)}`, zTop: l.zTopMm, zBottom: l.zBottomMm });
  if (silk.bottom) out.push({ key: 'silk:bottom', label: 'Silkscreen (bottom)', detail: 'ink on the mask', zTop: bottom, zBottom: bottom });
  const mid = (out.length - 1) / 2;
  return out.map((s, i) => ({ ...s, offset: (mid - i) * gap }));
}

/** How far each board of a system moves up (mm in z) while the stackups are apart, so that one
 * board's layers do not pass into another's: each board spreads about its own middle by `half`
 * each way, and a board above another moves up by half of each one's spread. Boards side by side
 * (at one height `z`, within a millimetre) move together, by the larger spread; the board `base`
 * stays where it is. */
export function spreadMembers(rows: { key: string; z: number; half: number }[], base: string): Map<string, number> {
  const out = new Map<string, number>();
  let shift = 0;
  let prev: { z: number; half: number } | null = null;
  for (const r of [...rows].sort((a, b) => a.z - b.z)) {
    const row = { z: r.z, half: r.half };
    if (prev && row.z - prev.z > 1) shift += prev.half + row.half;
    else if (prev) row.half = Math.max(row.half, prev.half);
    out.set(r.key, shift);
    prev = row;
  }
  const at = out.get(base) ?? 0;
  for (const [k, v] of out) out.set(k, v - at);
  return out;
}

/** The slot a face's parts lift with: the outermost on that face (its silkscreen, else its mask). */
export function faceSlot(slots: Slot[], face: Face): Slot | null {
  return (face === 'top' ? slots[0] : slots[slots.length - 1]) ?? null;
}

/** The role a layer's slab of the section is drawn in, as `netsmith render --section` colours it:
 * the mask, the core and the prepreg in their own colours, and a copper layer as the insulator that
 * fills round its copper (the mask for an outer layer, the prepreg for an inner one). */
export function slabRole(layers: StackLayer[], i: number): 'mask-top' | 'mask-bottom' | 'substrate' | 'prepreg' {
  const l = layers[i];
  const coppers = layers.map((x, k) => (x.kind === 'copper' ? k : -1)).filter((k) => k >= 0);
  if (l.kind === 'core') return 'substrate';
  if (l.kind === 'prepreg') return 'prepreg';
  const upper = i < layers.length / 2;
  if (l.kind === 'mask') return upper ? 'mask-top' : 'mask-bottom';
  if (i === coppers[0]) return 'mask-top';
  if (i === coppers[coppers.length - 1]) return 'mask-bottom';
  return 'prepreg';
}

/** Smooth at both ends, for the layers moving apart. */
export const easeInOut = (t: number): number => {
  const k = Math.min(1, Math.max(0, t));
  return k < 0.5 ? 4 * k * k * k : 1 - (-2 * k + 2) ** 3 / 2;
};

// ---- the view as a `netsmith render` command ----

export type RenderView = 'top' | 'bottom' | 'front' | 'back' | 'left' | 'right' | 'iso' | 'iso-bottom';

const K = 1 / Math.sqrt(3);

/** The direction from the target to the eye of each view of `netsmith render` (raster3d's
 * `View::direction`), in the scene's frame. */
export const RENDER_VIEWS: [RenderView, P3][] = [
  ['top', [0, 0, 1]],
  ['bottom', [0, 0, -1]],
  ['front', [0, -1, 0]],
  ['back', [0, 1, 0]],
  ['left', [-1, 0, 0]],
  ['right', [1, 0, 0]],
  ['iso', [K, -K, K]],
  ['iso-bottom', [K, -K, -K]],
];

/** The view of `netsmith render` nearest to a camera looking from `towardEye`, and whether it is
 * that view (within a few degrees) or only the nearest. */
export function nearestView(towardEye: P3): { view: RenderView; exact: boolean } {
  const l = Math.hypot(...towardEye) || 1;
  const d = towardEye.map((v) => v / l);
  let best: RenderView = 'iso';
  let dot = -2;
  for (const [v, dir] of RENDER_VIEWS) {
    const c = d[0] * dir[0] + d[1] * dir[1] + d[2] * dir[2];
    if (c > dot) [best, dot] = [v, c];
  }
  return { view: best, exact: dot > 0.995 };
}

/** A word for a shell: as it is when it is plain, else in single quotes. */
export function shellWord(s: string): string {
  if (/^[\w@%+=:,./-]+$/.test(s)) return s;
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

export interface RenderRequest {
  /** The board's source file, from the project's folder (`src/main.nsm`). */
  file: string;
  board: string;
  view: RenderView;
  section?: { axis: Axis; at: number } | null;
  /** The designators of the parts picked. */
  highlight?: string[];
  /** How the case model is drawn: named in the command (`--case`) when it is not see-through, the command's default. */
  case?: CaseMode;
  dark?: boolean;
  perspective?: boolean;
}

/** The `netsmith render` command that draws what the view shows: its view, its cut, the parts
 * picked lit, its theme and its lens. The size and the output are the command's defaults. */
export function renderCommand(r: RenderRequest): string {
  const words = ['netsmith', 'render', shellWord(r.file), '--board', shellWord(r.board), '--view', r.view];
  if (r.section) words.push('--section', `${r.section.axis}=${Number(r.section.at.toFixed(3))}`);
  const lit = [...new Set(r.highlight ?? [])].filter(Boolean);
  if (lit.length) words.push('--highlight', shellWord(lit.join(',')));
  if (r.case && r.case !== 'see-through') words.push('--case', r.case);
  if (r.dark) words.push('--theme', 'dark');
  if (r.perspective) words.push('--perspective');
  return words.join(' ');
}
