// What the schematic's SVG says, as data: which sheet each part, pin and net is on and where.
// The drawing is one flat SVG (a box for each module, parts and wires in separate groups), so
// the sheet of a thing is the box its position falls in. `readDrawing` takes it out of the SVG;
// `buildIndex` is pure.

import type { Box } from './zoom';
import { SCALE } from './zoom';

/** A module's box in the drawing, in canvas pixels. */
export interface Sheet {
  /** The module instance (`stage`); a cut sheet adds its number (`stage (2/3)`). */
  name: string;
  /** What the drawing prints on the box: `stage: LdoStage`. */
  title: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface RawPart {
  ref: string;
  box: Box;
}
export interface RawPin {
  ref: string;
  pin: string;
  net: string | null;
  x: number;
  y: number;
}
export interface RawNetMark {
  net: string;
  kind: 'wire' | 'junction' | 'port' | 'label' | 'power';
  box: Box;
  /** Where a flag points at the wire. */
  at: { x: number; y: number };
}
/** A member of a bus: its name in the interface (`tx`) and the net it is on. */
export interface BusMember {
  name: string;
  net: string;
}
/** A bus as the drawing says it (`g.bus-group`): the interface instance, its definition, its
 * members in the order of the pins, whether it ends in a flag (the members go on to another sheet)
 * and the box of its thick line, in canvas pixels. */
export interface RawBus {
  path: string;
  type: string;
  members: BusMember[];
  flag: boolean;
  box: Box;
}
/** The drawing as plain numbers, in canvas pixels. */
export interface RawDrawing {
  sheets: Sheet[];
  parts: RawPart[];
  pins: RawPin[];
  nets: RawNetMark[];
  buses: RawBus[];
}

/** A bus and the sheet it is drawn on. */
export interface Bus extends RawBus {
  sheet: string;
  /** The member nets, in the order of the members. */
  nets: string[];
}

/** A net's presence on one sheet. */
export interface NetPlace {
  box: Box;
  pins: number;
  wires: number;
  labels: number;
  /** The flags where it leaves this sheet. */
  flags: { x: number; y: number }[];
}

export interface DrawingIndex {
  /** The sheet each part is on (the empty name when the board is one page). */
  partSheet: Map<string, string>;
  partBox: Map<string, Box>;
  /** Where a pin ends, by `REF.PIN`. */
  pinPoint: Map<string, { x: number; y: number; sheet: string }>;
  /** For each net, its place on each sheet that draws it. */
  netPlaces: Map<string, Map<string, NetPlace>>;
  /** The buses drawn, in the drawing's order. */
  buses: Bus[];
}

/** The name of the sheet whose box has the point, or the empty name. */
export function sheetAt(sheets: Sheet[], x: number, y: number): string {
  for (const s of sheets) if (x >= s.x && x <= s.x + s.w && y >= s.y && y <= s.y + s.h) return s.name;
  return '';
}

const union = (a: Box | undefined, b: Box): Box => {
  if (!a) return { ...b };
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
};

export function buildIndex(raw: RawDrawing): DrawingIndex {
  const partSheet = new Map<string, string>();
  const partBox = new Map<string, Box>();
  for (const p of raw.parts) {
    partBox.set(p.ref, p.box);
    partSheet.set(p.ref, sheetAt(raw.sheets, p.box.x + p.box.w / 2, p.box.y + p.box.h / 2));
  }
  const pinPoint = new Map<string, { x: number; y: number; sheet: string }>();
  for (const p of raw.pins) {
    pinPoint.set(`${p.ref}.${p.pin}`, { x: p.x, y: p.y, sheet: partSheet.get(p.ref) ?? sheetAt(raw.sheets, p.x, p.y) });
  }
  const netPlaces = new Map<string, Map<string, NetPlace>>();
  const boxes = new Map<NetPlace, Box>();
  const place = (net: string, sheet: string): NetPlace => {
    let sheets = netPlaces.get(net);
    if (!sheets) netPlaces.set(net, (sheets = new Map()));
    let at = sheets.get(sheet);
    if (!at) sheets.set(sheet, (at = { box: { x: 0, y: 0, w: 0, h: 0 }, pins: 0, wires: 0, labels: 0, flags: [] }));
    return at;
  };
  const grow = (p: NetPlace, b: Box) => {
    const u = union(boxes.get(p), b);
    boxes.set(p, u);
    p.box = u;
  };
  for (const m of raw.nets) {
    const p = place(m.net, sheetAt(raw.sheets, m.box.x + m.box.w / 2, m.box.y + m.box.h / 2));
    grow(p, m.box);
    if (m.kind === 'wire') p.wires++;
    else if (m.kind === 'label') p.labels++;
    else if (m.kind === 'port') p.flags.push(m.at);
  }
  for (const pin of raw.pins) {
    if (!pin.net) continue;
    const p = place(pin.net, pinPoint.get(`${pin.ref}.${pin.pin}`)?.sheet ?? '');
    grow(p, { x: pin.x, y: pin.y, w: 0, h: 0 });
    p.pins++;
  }
  const buses: Bus[] = raw.buses.map((b) => ({
    ...b,
    sheet: sheetAt(raw.sheets, b.box.x + b.box.w / 2, b.box.y + b.box.h / 2),
    nets: b.members.map((m) => m.net),
  }));
  return { partSheet, partBox, pinPoint, netPlaces, buses };
}

const nums = (text: string | null): number[] => (text ?? '').split(/[\s,]+/).filter(Boolean).map(Number).filter((n) => Number.isFinite(n));

function boxOfPoints(points: number[]): Box | null {
  if (points.length < 2) return null;
  let x0 = Infinity,
    y0 = Infinity,
    x1 = -Infinity,
    y1 = -Infinity;
  for (let i = 0; i + 1 < points.length; i += 2) {
    x0 = Math.min(x0, points[i]);
    x1 = Math.max(x1, points[i]);
    y0 = Math.min(y0, points[i + 1]);
    y1 = Math.max(y1, points[i + 1]);
  }
  return { x: x0 * SCALE, y: y0 * SCALE, w: (x1 - x0) * SCALE, h: (y1 - y0) * SCALE };
}

/** The members a bus-group says: `data-members` is JSON, `[["tx","MODEM_TX"],...]`; a drawing that
 * has only `data-nets` (a space between the nets) names each member by its net. */
export function busMembers(members: string | null, nets: string | null): BusMember[] {
  try {
    const v: unknown = JSON.parse(members ?? '');
    if (Array.isArray(v)) {
      const out = v.filter((m): m is [string, string] => Array.isArray(m) && typeof m[0] === 'string' && typeof m[1] === 'string');
      if (out.length === v.length) return out.map(([name, net]) => ({ name, net }));
    }
  } catch {
    // Not JSON: the older attribute is read.
  }
  return (nets ?? '').split(' ').filter(Boolean).map((net) => ({ name: net, net }));
}

/** The box of a bus's thick line (the runs of its path, `M x y L x y ...`) and of its flag, in
 * canvas pixels, or null for a bus with neither. */
export function busBox(paths: string[], flagPoints: string | null): Box | null {
  let box: Box | null = null;
  const add = (b: Box | null) => {
    if (b) box = box ? union(box, b) : b;
  };
  for (const d of paths) add(boxOfPoints(nums(d.replace(/[A-Za-z]/g, ' '))));
  add(flagPoints === null ? null : boxOfPoints(nums(flagPoints)));
  return box;
}

const num = (e: Element, name: string) => Number(e.getAttribute(name) ?? 0);

/** Read a schematic SVG (any element tree with the DOM's query methods) into plain numbers. The
 * drawing's units are millimetres; the numbers here are canvas pixels (`SCALE` to the millimetre),
 * with the top left of the drawing's bounds at 0, 0. */
export function readDrawing(svg: Element, origin: { x: number; y: number }): RawDrawing {
  const ox = origin.x * SCALE;
  const oy = origin.y * SCALE;
  const shift = (b: Box): Box => ({ x: b.x - ox, y: b.y - oy, w: b.w, h: b.h });

  const sheets = [...svg.querySelectorAll('.module-sheet')].map((g) => {
    const [x, y, w, h] = nums(g.getAttribute('data-bounds'));
    return {
      name: g.getAttribute('data-module') ?? '',
      title: g.querySelector('.module-title')?.textContent ?? '',
      x: x * SCALE - ox,
      y: y * SCALE - oy,
      w: w * SCALE,
      h: h * SCALE,
    };
  });

  const parts: RawPart[] = [];
  const pins: RawPin[] = [];
  for (const g of svg.querySelectorAll('.part[data-ref]')) {
    const ref = g.getAttribute('data-ref')!;
    let box: Box | undefined;
    const add = (b: Box | null) => {
      if (b) box = union(box, b);
    };
    for (const pin of g.querySelectorAll('.pin')) {
      const end = pin.querySelector('.pin-end');
      if (!end) continue;
      const x = num(end, 'cx') * SCALE;
      const y = num(end, 'cy') * SCALE;
      pins.push({ ref, pin: pin.getAttribute('data-pin') ?? '', net: pin.getAttribute('data-net'), x: x - ox, y: y - oy });
      add({ x, y, w: 0, h: 0 });
    }
    for (const r of g.querySelectorAll('rect')) {
      add({ x: num(r, 'x') * SCALE, y: num(r, 'y') * SCALE, w: num(r, 'width') * SCALE, h: num(r, 'height') * SCALE });
    }
    for (const l of g.querySelectorAll('polyline')) add(boxOfPoints(nums(l.getAttribute('points'))));
    for (const c of g.querySelectorAll('circle')) {
      if (c.classList.contains('pin-end')) continue;
      const r = num(c, 'r');
      add({ x: (num(c, 'cx') - r) * SCALE, y: (num(c, 'cy') - r) * SCALE, w: 2 * r * SCALE, h: 2 * r * SCALE });
    }
    if (box) parts.push({ ref, box: shift(box) });
  }

  const nets: RawNetMark[] = [];
  const mark = (net: string, kind: RawNetMark['kind'], b: Box | null, at?: { x: number; y: number }) => {
    if (!b) return;
    const box = shift(b);
    nets.push({ net, kind, box, at: at ? { x: at.x - ox, y: at.y - oy } : { x: box.x + box.w / 2, y: box.y + box.h / 2 } });
  };
  for (const g of svg.querySelectorAll('.wire[data-net]')) {
    const line = g.querySelector('polyline.line') ?? g.querySelector('polyline');
    mark(g.getAttribute('data-net')!, 'wire', line ? boxOfPoints(nums(line.getAttribute('points'))) : null);
  }
  for (const c of svg.querySelectorAll('circle.junction[data-net]')) {
    mark(c.getAttribute('data-net')!, 'junction', { x: num(c, 'cx') * SCALE, y: num(c, 'cy') * SCALE, w: 0, h: 0 });
  }
  for (const g of svg.querySelectorAll('.port[data-net]')) {
    const points = nums(g.querySelector('polygon')?.getAttribute('points') ?? null);
    mark(g.getAttribute('data-net')!, 'port', boxOfPoints(points), points.length >= 2 ? { x: points[0] * SCALE, y: points[1] * SCALE } : undefined);
  }
  for (const g of svg.querySelectorAll('.power[data-net]')) {
    const line = g.querySelector('polyline');
    mark(g.getAttribute('data-net')!, 'power', line ? boxOfPoints(nums(line.getAttribute('points'))) : null);
  }
  for (const t of svg.querySelectorAll('text.net-label[data-net]')) {
    mark(t.getAttribute('data-net')!, 'label', { x: num(t, 'x') * SCALE, y: num(t, 'y') * SCALE, w: 0, h: 0 });
  }
  const buses: RawBus[] = [];
  for (const g of svg.querySelectorAll('.bus-group[data-bus]')) {
    const flag = g.querySelector('.bus-flag polygon');
    const box = busBox(
      [...g.querySelectorAll('.bus path.line')].map((p) => p.getAttribute('d') ?? ''),
      flag?.getAttribute('points') ?? null,
    );
    if (!box) continue;
    buses.push({
      path: g.getAttribute('data-bus')!,
      type: g.getAttribute('data-type') ?? '',
      members: busMembers(g.getAttribute('data-members'), g.getAttribute('data-nets')),
      flag: !!flag,
      box: shift(box),
    });
  }
  return { sheets, parts, pins, nets, buses };
}
