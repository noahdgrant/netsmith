// What the 3D view reads from the scene's `.glb` file: the ids and facts in each node's
// `extras.netsmith` (the contract of docs/BOARD3D.md, schema 1), as plain records, and the words
// the view says about them. The file is written by the Rust side (crates/netsmith-pcb/src/gltf.rs);
// the page assumes no node beyond what the contract names, and a node of a kind it does not know
// (copper, later shapes) is drawn and otherwise left alone. Nothing here touches three.js.

import type { Provenance } from './palette';

export type Face = 'top' | 'bottom';

export interface Finding3d {
  code: string;
  message: string;
  parts: string[];
  limit?: string;
  face?: Face;
}

export interface BoardInfo {
  kind: 'board';
  schema: number;
  board: string;
  widthMm: number;
  heightMm: number;
  thicknessMm: number;
  coverage: { known: number; total: number };
  findings: Finding3d[];
}

export interface PartInfo3d {
  kind: 'part';
  path: string;
  refdes: string;
  side: Face;
  provenance: Provenance;
  /** Null when no height is known: never zero, never a guess. */
  heightMm: number | null;
  /** The part's kind as the design names it (`Resistor`), and what sort of part it is (`resistor`, `ic`...); empty and null in a file that has none. */
  partKind: string;
  category: string | null;
  /** The least margin to a height limit over the part, and that limit; null when none is over it or its height is unknown. */
  marginMm: number | null;
  marginLimit: string | null;
}

/** A net's copper on one layer (`copper:<layer>:<net>`), or a via's barrel (`via:<net>`). A net of null is the copper with none. */
export interface CopperInfo {
  kind: 'copper' | 'hole';
  layer: string | null;
  net: string | null;
  /** A pad's barrel (`hole:<refdes>.<pad>`): its part's path and its pad; its net is the pad's, which the file does not say. */
  path?: string;
  pad?: string;
}

export interface LimitInfo {
  kind: 'height_limit';
  name: string;
  face: Face;
  maxHeightMm: number;
  violated: boolean;
  marginMm: number | null;
  tallest: string | null;
  unknown: string[];
  /** Every part under it on its face (an older file has none). */
  under: string[];
}

export type Info = BoardInfo | PartInfo3d | LimitInfo | CopperInfo | { kind: string };

const num = (v: unknown, or = 0): number => (typeof v === 'number' && Number.isFinite(v) ? v : or);
const numOrNull = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const str = (v: unknown): string => (typeof v === 'string' ? v : '');
const face = (v: unknown): Face => (v === 'bottom' ? 'bottom' : 'top');
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);

/** The node's facts from what three's loader puts in `userData` (the node's `extras`), or null for a node with none. */
export function readInfo(userData: unknown): Info | null {
  const n = (userData as { netsmith?: Record<string, unknown> } | null)?.netsmith;
  if (!n || typeof n !== 'object' || typeof n.kind !== 'string') return null;
  switch (n.kind) {
    case 'board': {
      const c = (n.coverage ?? {}) as Record<string, unknown>;
      return {
        kind: 'board',
        schema: num(n.schema, 1),
        board: str(n.board),
        widthMm: num(n.width_mm),
        heightMm: num(n.height_mm),
        thicknessMm: num(n.thickness_mm),
        coverage: { known: num(c.known), total: num(c.total) },
        findings: (Array.isArray(n.findings) ? n.findings : []).map((f: Record<string, unknown>) => ({
          code: str(f.code),
          message: str(f.message),
          parts: strings(f.parts),
          ...(typeof f.limit === 'string' ? { limit: f.limit } : {}),
          ...(f.face === 'top' || f.face === 'bottom' ? { face: f.face } : {}),
        })),
      };
    }
    case 'part': {
      const p = n.provenance;
      return {
        kind: 'part',
        path: str(n.path),
        refdes: str(n.refdes),
        side: face(n.side),
        provenance: p === 'stated' || p === 'catalog' ? p : 'unknown',
        heightMm: numOrNull(n.height_mm),
        partKind: str(n.part_kind),
        category: typeof n.category === 'string' ? n.category : null,
        marginMm: numOrNull(n.margin_mm),
        marginLimit: typeof n.margin_limit === 'string' ? n.margin_limit : null,
      };
    }
    case 'copper':
    case 'hole':
      return {
        kind: n.kind,
        layer: typeof n.layer === 'string' ? n.layer : null,
        net: typeof n.net === 'string' ? n.net : null,
        ...(typeof n.path === 'string' && typeof n.pad === 'string' ? { path: n.path, pad: n.pad } : {}),
      };
    case 'height_limit':
      return {
        kind: 'height_limit',
        name: str(n.name),
        face: face(n.face),
        maxHeightMm: num(n.max_height_mm),
        violated: n.violated === true,
        marginMm: numOrNull(n.margin_mm),
        tallest: typeof n.tallest === 'string' ? n.tallest : null,
        unknown: strings(n.unknown),
        under: strings(n.under),
      };
    default:
      return { kind: n.kind };
  }
}

/** A length in mm for a person: at most two decimals, no trailing zeros. */
export const mm = (v: number): string => `${Number(v.toFixed(2))} mm`;

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The coverage chip: how many of the parts have a known height. */
export function coverageLine(c: { known: number; total: number }): string {
  if (c.total === 0) return 'No parts with a body';
  if (c.known === c.total) return c.total === 1 ? 'The part has a known height' : `All ${c.total} parts have a known height`;
  return `${c.known} of ${plural(c.total, 'part')} ${c.known === 1 ? 'has' : 'have'} a known height`;
}

/** Where a height came from, in words. */
export const PROVENANCE_WORDS: Record<Provenance, string> = {
  stated: 'stated in the design',
  catalog: 'from the catalog row',
  unknown: 'not known',
};

/** The height of a part and where it came from, in one line. */
export function heightLine(p: Pick<PartInfo3d, 'heightMm' | 'provenance'>): string {
  if (p.heightMm === null || p.provenance === 'unknown') return 'Height not known: drawn flat and hatched, not guessed';
  return `${mm(p.heightMm)} tall, ${PROVENANCE_WORDS[p.provenance]}`;
}

/** What the hover tooltip says of a part: its designator (and value), its height and its source, its face, and the 3D findings about it. */
export function partTip(p: PartInfo3d, findings: Finding3d[], value = ''): { title: string; lines: string[] } {
  const lines = [heightLine(p), `On the ${p.side} face${p.partKind ? ` · ${p.partKind}` : ''}`];
  if (p.marginMm !== null && p.marginMm >= 0 && p.marginLimit) lines.push(`${mm(p.marginMm)} under the limit ${p.marginLimit}`);
  for (const f of findings) {
    if (!f.parts.includes(p.path)) continue;
    const limit = f.limit ? `the limit ${f.limit}` : 'a height limit';
    if (f.code === 'too_tall') lines.push(`Too tall for ${limit}`);
    else if (f.code === 'height_unchecked') lines.push(`Under ${limit}, which is not checked over it`);
    else if (f.code === 'body_overlap') lines.push('Its body overlaps another');
  }
  return { title: `${p.refdes}${value ? ` ${value}` : ''}`, lines };
}

/** What the tooltip says of a height limit's volume. */
export function limitTip(l: LimitInfo): { title: string; lines: string[] } {
  const lines = [`Parts at most ${mm(l.maxHeightMm)} tall on the ${l.face} face`];
  if (l.violated && l.marginMm !== null) lines.push(`A part is ${mm(-l.marginMm)} too tall`);
  else if (l.marginMm !== null) lines.push(`${mm(l.marginMm)} to spare over the tallest part`);
  if (l.unknown.length) lines.push(`${plural(l.unknown.length, 'part')} of unknown height under it, not checked`);
  return { title: `Height limit ${l.name}`, lines };
}

/** The parts of unknown height, by designator, for the coverage list. */
export function unknownParts(parts: PartInfo3d[]): PartInfo3d[] {
  return parts.filter((p) => p.provenance === 'unknown').sort((a, b) => a.refdes.localeCompare(b.refdes, undefined, { numeric: true }));
}

/** How many parts' heights came from each source. */
export function provenanceCounts(parts: PartInfo3d[]): Record<Provenance, number> {
  const out: Record<Provenance, number> = { stated: 0, catalog: 0, unknown: 0 };
  for (const p of parts) out[p.provenance]++;
  return out;
}
