// The pure side of the 3D view's ways of looking (inspectMode.ts has the state): the colours of
// kind and of height margin, which materials x-ray makes see-through, which nets are lit, the
// geometry of the height preview, and what a finding points at. Nothing here touches three.js, so
// vitest holds it; viewer.ts draws with it.

import type { LimitInfo, PartInfo3d } from './model';
import type { Role } from './palette';

// ---- colour by kind ----

/** The swatches of colour by kind. A fuse, a mechanical part and any other part share `other`. */
export type KindSwatch = 'resistor' | 'capacitor' | 'inductor' | 'diode' | 'transistor' | 'crystal' | 'ic' | 'connector' | 'other';

export const KINDS: { id: KindSwatch; label: string; token: string }[] = [
  { id: 'ic', label: 'IC', token: '--pcb3d-kind-ic' },
  { id: 'connector', label: 'Connector', token: '--pcb3d-kind-connector' },
  { id: 'resistor', label: 'Resistor', token: '--pcb3d-kind-resistor' },
  { id: 'capacitor', label: 'Capacitor', token: '--pcb3d-kind-capacitor' },
  { id: 'inductor', label: 'Inductor', token: '--pcb3d-kind-inductor' },
  { id: 'diode', label: 'Diode', token: '--pcb3d-kind-diode' },
  { id: 'transistor', label: 'Transistor', token: '--pcb3d-kind-transistor' },
  { id: 'crystal', label: 'Crystal', token: '--pcb3d-kind-crystal' },
  { id: 'other', label: 'Other', token: '--pcb3d-kind-other' },
];

/** The swatch of a part's `category` (the file's word, docs/BOARD3D.md); a file with none, or a word the page does not know, is `other`. */
export function kindSwatch(category: string | null | undefined): KindSwatch {
  return KINDS.some((k) => k.id === category) ? (category as KindSwatch) : 'other';
}

/** How many parts of each swatch, in the legend's order, leaving out the swatches with none. Parts of unknown height are counted: they stay hatched, but are of a kind. */
export function kindCounts(parts: PartInfo3d[]): { id: KindSwatch; label: string; count: number }[] {
  const n = new Map<KindSwatch, number>();
  for (const p of parts) n.set(kindSwatch(p.category), (n.get(kindSwatch(p.category)) ?? 0) + 1);
  return KINDS.filter((k) => n.has(k.id)).map((k) => ({ id: k.id, label: k.label, count: n.get(k.id)! }));
}

// ---- colour by height margin (a height map) ----

/**
 * "Plenty to spare" on the height map, in mm: a body whose margin to the height limit over it is
 * at least this much is green; from no margin up to it, amber blending to green; below no margin
 * (too tall), red. It is a choice of the picture, not a rule of the design: no check reads it, and
 * the margins themselves are the file's (`margin_mm`). The legend says it.
 */
export const MARGIN_EASY_MM = 1;

export type MarginBand = 'over' | 'tight' | 'easy' | 'none';

/** Where a margin falls: `none` when no limit is over the part or its height is unknown. */
export function marginBand(m: number | null): MarginBand {
  if (m === null) return 'none';
  if (m < 0) return 'over';
  return m < MARGIN_EASY_MM ? 'tight' : 'easy';
}

export const MARGIN_TOKENS = {
  easy: '--pcb3d-margin-easy',
  tight: '--pcb3d-margin-tight',
  over: '--pcb3d-margin-over',
  none: '--pcb3d-margin-none',
} as const;

type Rgb = [number, number, number];
const channels = (hex: string): Rgb => {
  const h = parseInt(hex.replace('#', ''), 16);
  return [(h >> 16) & 255, (h >> 8) & 255, h & 255];
};
const hexOf = (c: Rgb) => `#${c.map((v) => Math.round(v).toString(16).padStart(2, '0')).join('')}`;
const mix = (a: Rgb, b: Rgb, t: number): Rgb => [0, 1, 2].map((i) => a[i] + (b[i] - a[i]) * t) as Rgb;

/** A body's colour on the height map: red when too tall, amber at no margin blending to green at {@link MARGIN_EASY_MM}, grey where there is no margin. */
export function marginColour(m: number | null, c: Record<MarginBand, string>): string {
  const band = marginBand(m);
  if (band !== 'tight') return c[band];
  return hexOf(mix(channels(c.tight), channels(c.easy), Math.max(0, Math.min(1, m! / MARGIN_EASY_MM))));
}

// ---- x-ray, and the dimming of a lit net ----

/** How see-through x-ray makes each role, or null for one it leaves alone (the copper, the gold and the hatched unknowns, which are what x-ray is for, and the limits, see-through already). */
export function xrayOpacity(role: Role): number | null {
  switch (role) {
    case 'substrate':
    case 'prepreg':
    // A solid case is see-through in x-ray, as the see-through one is always.
    case 'case-solid':
      return 0.16;
    case 'mask-top':
    case 'mask-bottom':
    case 'mask-over-copper':
      return 0.14;
    case 'body':
    case 'body-catalog':
    case 'body-standard':
      return 0.26;
    case 'metal':
      return 0.32;
    case 'mark':
    case 'silk':
      return 0.25;
    default:
      return null;
  }
}

/** How a lit net leaves the rest of the copper: this much of the background mixed in, and this see-through. */
export const DIM = { toward: 0.75, opacity: 0.32 } as const;

export type NetState = 'select' | 'hover';

/** The nets to light, and how: the page's picked net in the selection colour, the hovered ones (a net row, a pad, a part's nets) lit, a pick winning over a hover. */
export function litNets(picked: string | null, hovered: Iterable<string>): Map<string, NetState> {
  const out = new Map<string, NetState>();
  for (const n of hovered) out.set(n, 'hover');
  if (picked) out.set(picked, 'select');
  return out;
}

/** A piece of copper or a barrel as the view knows it: its own net (copper, a via), or its part's path and pad (a pad's barrel). */
export interface Piece {
  kind: string;
  layer: string | null;
  net: string | null;
  path?: string;
  pad?: string;
}

/** A piece's net: its own, else its pad's (`path:pad` in `padNets`, from the 2D scene). */
export function pieceNet(p: Piece, padNets: ReadonlyMap<string, string>): string | null {
  return p.net ?? (p.path && p.pad ? (padNets.get(`${p.path}:${p.pad}`) ?? null) : null);
}

/** What the lit nets light: the copper layers that have their copper, the barrels (vias and pads' holes) and the vias alone, and how many other pieces are dimmed. */
export function litReport(pieces: Piece[], nets: ReadonlyMap<string, NetState>, padNets: ReadonlyMap<string, string>): { layers: string[]; barrels: number; vias: number; dimmed: number } {
  const lit = pieces.filter((p) => {
    const n = pieceNet(p, padNets);
    return n !== null && nets.has(n);
  });
  return {
    layers: [...new Set(lit.filter((p) => p.kind === 'copper').map((p) => p.layer ?? ''))].sort(),
    barrels: lit.filter((p) => p.kind === 'hole').length,
    vias: lit.filter((p) => p.kind === 'hole' && p.net !== null).length,
    dimmed: nets.size ? pieces.length - lit.length : 0,
  };
}

// ---- the height preview ----

/**
 * A flat outline (a part of unknown height, as the file draws it: triangles in one plane of
 * constant y, glTF's up) stood up as a solid from `y0` to `y1`: the outline at both ends and a
 * wall along each edge that only one triangle has (the outline's boundary). Positions in, flat
 * triangles out, in the same units. For the picture only: nothing else reads it.
 */
export function extrudeFlat(pos: ArrayLike<number>, index: ArrayLike<number> | null, y0: number, y1: number): Float32Array {
  const tri: number[] = [];
  if (index) for (let i = 0; i < index.length; i++) tri.push(index[i]);
  else for (let i = 0; i < pos.length / 3; i++) tri.push(i);
  const key = (i: number) => `${pos[i * 3].toFixed(7)},${pos[i * 3 + 2].toFixed(7)}`;
  const xz = (i: number): [number, number] => [pos[i * 3], pos[i * 3 + 2]];
  const out: number[] = [];
  const put = (p: [number, number], y: number) => out.push(p[0], y, p[1]);
  // Edges by their two points, either way round: one used once is on the boundary.
  const edges = new Map<string, { a: number; b: number; n: number }>();
  for (let t = 0; t + 2 < tri.length; t += 3) {
    const v = [tri[t], tri[t + 1], tri[t + 2]];
    for (const y of [y0, y1]) for (const i of v) put(xz(i), y);
    for (let k = 0; k < 3; k++) {
      const a = v[k];
      const b = v[(k + 1) % 3];
      const [ka, kb] = [key(a), key(b)];
      const id = ka < kb ? `${ka}|${kb}` : `${kb}|${ka}`;
      const e = edges.get(id);
      if (e) e.n++;
      else edges.set(id, { a, b, n: 1 });
    }
  }
  for (const e of edges.values()) {
    if (e.n !== 1) continue;
    const [pa, pb] = [xz(e.a), xz(e.b)];
    put(pa, y0);
    put(pb, y0);
    put(pb, y1);
    put(pa, y0);
    put(pb, y1);
    put(pa, y1);
  }
  return new Float32Array(out);
}

// ---- a finding, in 3D ----

/** What a finding of the problems dock is about in the scene: its parts by path (from their designators) and the faces of the limit it names, as `name:face`. */
export function findingTargets(refs: string[], keepout: string | null, parts: PartInfo3d[], limits: LimitInfo[]): { paths: string[]; limits: string[] } {
  const paths = parts.filter((p) => refs.includes(p.refdes)).map((p) => p.path);
  const faces = new Set(parts.filter((p) => paths.includes(p.path)).map((p) => p.side));
  const named = limits.filter((l) => l.name === keepout);
  // The face the parts are on, where the limit has it; else every face of the limit.
  const onFace = named.filter((l) => faces.has(l.face));
  return { paths, limits: (onFace.length ? onFace : named).map((l) => `${l.name}:${l.face}`) };
}

/** The tokens of this file, for the test that every theme has them. */
export function inspectTokens(): string[] {
  return [...KINDS.map((k) => k.token), ...Object.values(MARGIN_TOKENS), '--pcb3d-preview'];
}
