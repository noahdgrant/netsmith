// The layout of the power tree: a left-to-right hierarchy. Rails (cards) and the stages between
// them (converters, protection) are nodes in layers, every layer to the right of what feeds it;
// an edge that crosses layers passes through a lane of its own, so no edge runs through a node.
// Edges are right angles with rounded corners. Nothing is random: ids are sorted, ties are broken
// by name, and the same tree always gives the same drawing.

export interface Item {
  id: string;
  kind: 'rail' | 'stage';
  w: number;
  h: number;
  /** Where edges meet the node, from its top: the middle of a stage, the header of a rail. */
  anchor: number;
}

export interface Link {
  from: string;
  to: string;
}

export interface Placed extends Item {
  x: number;
  y: number;
  layer: number;
  /** Its place in its layer, top to bottom, among the nodes that are not lanes. */
  order: number;
}

export interface Routed {
  from: string;
  to: string;
  /** The SVG path of the whole edge. */
  d: string;
  /** The corners of the path, left to right. */
  points: [number, number][];
}

export interface TreeLayout {
  nodes: Placed[];
  edges: Routed[];
  width: number;
  height: number;
  /** The nodes of each layer, top to bottom. */
  layers: string[][];
}

export const PAD = 36;
/** The space between nodes of a layer. */
export const GAP_Y = 26;
/** The least space between two layers, for the edges between them. */
export const GAP_X = 44;
/** The space one edge's vertical run takes in the gap. */
const LANE = 14;
/** The height of the lane a long edge keeps in a layer it passes. */
const THROUGH = 10;
const CORNER = 7;
const SWEEPS = 6;

interface Slot {
  id: string;
  dummy: boolean;
  w: number;
  h: number;
  anchor: number;
}

/** Lay out `items` joined by `links`. Links whose ends are not items are left out; a link that
 * would close a cycle is left out of the layering and drawn backwards, like any other. */
export function layoutTree(items: Item[], links: Link[]): TreeLayout {
  const sorted = [...items].sort((a, b) => a.id.localeCompare(b.id));
  const byId = new Map(sorted.map((i) => [i.id, i]));
  const real = links
    .filter((l) => byId.has(l.from) && byId.has(l.to) && l.from !== l.to)
    .sort((a, b) => a.from.localeCompare(b.from) || a.to.localeCompare(b.to))
    .filter((l, i, all) => i === 0 || l.from !== all[i - 1].from || l.to !== all[i - 1].to);

  // Layers: the longest path from a source, with the links that close a cycle left out.
  const out = new Map<string, string[]>(sorted.map((i) => [i.id, []]));
  for (const l of real) out.get(l.from)!.push(l.to);
  const state = new Map<string, number>();
  const forward: Link[] = [];
  const visit = (a: string) => {
    state.set(a, 1);
    for (const b of out.get(a)!) {
      if (state.get(b) === 1) continue;
      forward.push({ from: a, to: b });
      if (!state.has(b)) visit(b);
    }
    state.set(a, 2);
  };
  for (const i of sorted) if (!state.has(i.id)) visit(i.id);
  const layer = new Map<string, number>(sorted.map((i) => [i.id, 0]));
  const incoming = new Map<string, number>(sorted.map((i) => [i.id, 0]));
  for (const l of forward) incoming.set(l.to, incoming.get(l.to)! + 1);
  const forwardOut = new Map<string, string[]>(sorted.map((i) => [i.id, []]));
  for (const l of forward) forwardOut.get(l.from)!.push(l.to);
  const queue = sorted.filter((i) => incoming.get(i.id) === 0).map((i) => i.id);
  while (queue.length) {
    const a = queue.shift()!;
    for (const b of forwardOut.get(a)!) {
      layer.set(b, Math.max(layer.get(b)!, layer.get(a)! + 1));
      incoming.set(b, incoming.get(b)! - 1);
      if (incoming.get(b) === 0) queue.push(b);
    }
  }

  // Slots: the nodes, and a lane in each layer that a long edge passes.
  const slots = new Map<string, Slot>(sorted.map((i) => [i.id, { id: i.id, dummy: false, w: i.w, h: i.h, anchor: i.anchor }]));
  const layerOf = new Map(layer);
  /** For each forward link, the slots it passes through, ends included. */
  const chains: { link: Link; ids: string[] }[] = [];
  for (const l of forward) {
    const ids = [l.from];
    for (let k = layer.get(l.from)! + 1; k < layer.get(l.to)!; k++) {
      const id = `~${l.from}>${l.to}@${k}`;
      slots.set(id, { id, dummy: true, w: THROUGH, h: THROUGH, anchor: THROUGH / 2 });
      layerOf.set(id, k);
      ids.push(id);
    }
    ids.push(l.to);
    chains.push({ link: l, ids });
  }
  const depth = Math.max(-1, ...[...layerOf.values()]);
  const layers: string[][] = Array.from({ length: depth + 1 }, () => []);
  for (const id of [...sorted.map((i) => i.id), ...[...slots.keys()].filter((k) => k.startsWith('~'))]) layers[layerOf.get(id)!].push(id);

  // The links between neighbouring layers.
  const down = new Map<string, string[]>();
  const up = new Map<string, string[]>();
  for (const id of slots.keys()) {
    down.set(id, []);
    up.set(id, []);
  }
  for (const c of chains) {
    for (let k = 0; k + 1 < c.ids.length; k++) {
      down.get(c.ids[k])!.push(c.ids[k + 1]);
      up.get(c.ids[k + 1])!.push(c.ids[k]);
    }
  }

  // Order within layers: the mean place of the neighbours, swept back and forth, keeping the
  // order with the fewest crossings.
  const position = new Map<string, number>();
  const place = () => layers.forEach((l) => l.forEach((id, i) => position.set(id, i)));
  place();
  let best = layers.map((l) => [...l]);
  let fewest = crossings(layers, down);
  const reorder = (k: number, from: Map<string, string[]>) => {
    const key = new Map<string, number>();
    for (const id of layers[k]) {
      const n = from.get(id)!;
      key.set(id, n.length ? n.reduce((s, m) => s + position.get(m)!, 0) / n.length : position.get(id)!);
    }
    layers[k].sort((a, b) => key.get(a)! - key.get(b)! || position.get(a)! - position.get(b)! || a.localeCompare(b));
    layers[k].forEach((id, i) => position.set(id, i));
  };
  for (let s = 0; s < SWEEPS; s++) {
    for (let k = 1; k < layers.length; k++) reorder(k, up);
    for (let k = layers.length - 2; k >= 0; k--) reorder(k, down);
    const c = crossings(layers, down);
    if (c < fewest) {
      fewest = c;
      best = layers.map((l) => [...l]);
    }
  }
  best.forEach((l, k) => (layers[k] = l));
  place();

  // Vertical places: each layer stacked in its order, as near as it can to the places of its
  // neighbours, then a pass in each direction.
  const top = new Map<string, number>();
  for (const l of layers) {
    let y = PAD;
    for (const id of l) {
      top.set(id, y);
      y += slots.get(id)!.h + GAP_Y;
    }
  }
  const gapOf = (id: string) => (slots.get(id)!.dummy ? 8 : GAP_Y);
  const settle = (k: number, neighbours: Map<string, string[]>[]) => {
    const l = layers[k];
    const want = l.map((id) => {
      const n = neighbours.flatMap((m) => m.get(id)!);
      const slot = slots.get(id)!;
      return n.length ? n.reduce((s, m) => s + top.get(m)! + slots.get(m)!.anchor, 0) / n.length - slot.anchor : top.get(id)!;
    });
    // Isotonic regression: keep the order and the gaps, move as little as possible.
    const offset: number[] = [];
    let acc = 0;
    l.forEach((id, i) => {
      offset.push(acc);
      acc += slots.get(id)!.h + Math.max(gapOf(id), i + 1 < l.length ? gapOf(l[i + 1]) : 0);
    });
    const blocks: { sum: number; n: number }[] = [];
    l.forEach((_, i) => {
      blocks.push({ sum: want[i] - offset[i], n: 1 });
      while (blocks.length > 1) {
        const b = blocks[blocks.length - 1];
        const a = blocks[blocks.length - 2];
        if (a.sum / a.n <= b.sum / b.n) break;
        blocks.splice(blocks.length - 2, 2, { sum: a.sum + b.sum, n: a.n + b.n });
      }
    });
    let i = 0;
    for (const b of blocks) {
      for (let j = 0; j < b.n; j++, i++) top.set(l[i], b.sum / b.n + offset[i]);
    }
  };
  for (let pass = 0; pass < 3; pass++) {
    for (let k = 1; k < layers.length; k++) settle(k, [up]);
    for (let k = layers.length - 2; k >= 0; k--) settle(k, [down]);
  }
  for (let k = 0; k < layers.length; k++) settle(k, [up, down]);
  const floor = Math.min(...[...top.values()], PAD);
  for (const id of top.keys()) top.set(id, top.get(id)! - floor + PAD);

  // Ports: where an edge meets a node, spread along its anchor, in the order of the other ends.
  const portY = (id: string, other: string, side: 'out' | 'in') => {
    const slot = slots.get(id)!;
    const list = [...(side === 'out' ? down : up).get(id)!].sort(
      (a, b) => top.get(a)! + slots.get(a)!.anchor - (top.get(b)! + slots.get(b)!.anchor) || a.localeCompare(b),
    );
    const step = Math.min(9, Math.max(3, (slot.h - 2 * slot.anchor > 0 ? slot.anchor * 1.2 : slot.h) / Math.max(1, list.length)));
    const i = list.indexOf(other);
    return top.get(id)! + slot.anchor + (i - (list.length - 1) / 2) * (slot.dummy ? 0 : step);
  };

  // The gaps: each edge that changes height between two layers takes a lane of its own in the gap.
  type Seg = { a: string; b: string; ya: number; yb: number; lane: number };
  const segs: Seg[][] = Array.from({ length: Math.max(0, layers.length - 1) }, () => []);
  for (const c of chains) {
    for (let k = 0; k + 1 < c.ids.length; k++) {
      const a = c.ids[k];
      const b = c.ids[k + 1];
      segs[layerOf.get(a)!].push({ a, b, ya: portY(a, b, 'out'), yb: portY(b, a, 'in'), lane: 0 });
    }
  }
  const lanes = segs.map((g) => {
    const moving = g.filter((s) => Math.abs(s.ya - s.yb) > 0.5).sort((p, q) => Math.min(p.ya, p.yb) - Math.min(q.ya, q.yb) || p.a.localeCompare(q.a));
    const ends: number[] = [];
    for (const s of moving) {
      const lo = Math.min(s.ya, s.yb);
      const hi = Math.max(s.ya, s.yb);
      let k = ends.findIndex((e) => e < lo - 4);
      if (k < 0) k = ends.length;
      ends[k] = hi;
      s.lane = k;
    }
    return Math.max(1, ends.length);
  });

  // Horizontal places: a layer is as wide as its widest node; a gap as wide as its lanes.
  const left: number[] = [];
  const width: number[] = layers.map((l) => Math.max(0, ...l.map((id) => slots.get(id)!.w)));
  let x = PAD;
  const gaps: number[] = [];
  layers.forEach((_, k) => {
    left.push(x);
    const gap = k + 1 < layers.length ? Math.max(GAP_X, (lanes[k] + 1) * LANE) : 0;
    gaps.push(gap);
    x += width[k] + gap;
  });

  const nodes: Placed[] = [];
  layers.forEach((l, k) => {
    let order = 0;
    for (const id of l) {
      const slot = slots.get(id)!;
      if (slot.dummy) continue;
      const item = byId.get(id)!;
      nodes.push({ ...item, x: left[k], y: top.get(id)!, layer: k, order: order++ });
    }
  });

  const edges: Routed[] = chains.map((c) => {
    const pts: [number, number][] = [];
    for (let k = 0; k + 1 < c.ids.length; k++) {
      const a = c.ids[k];
      const b = c.ids[k + 1];
      const la = layerOf.get(a)!;
      const s = segs[la].find((q) => q.a === a && q.b === b)!;
      const xa = left[la] + (slots.get(a)!.dummy ? THROUGH : slots.get(a)!.w);
      const xb = left[la + 1];
      const gapX = left[la] + width[la];
      const xm = gapX + ((s.lane + 1) / (lanes[la] + 1)) * gaps[la];
      if (k === 0) pts.push([left[la] + slots.get(a)!.w, s.ya]);
      if (slots.get(a)!.dummy && k > 0) pts.push([xa, s.ya]);
      if (Math.abs(s.ya - s.yb) > 0.5) pts.push([xm, s.ya], [xm, s.yb]);
      pts.push([xb, s.yb]);
      if (slots.get(b)!.dummy) pts.push([xb + THROUGH, s.yb]);
    }
    return { from: c.link.from, to: c.link.to, d: roundedPath(pts), points: pts };
  });
  // A link that closes a cycle is not in a layer's order; it is drawn from the right of its
  // source to the left of its target like the others.
  const drawn = new Set(forward.map((l) => `${l.from}>${l.to}`));
  const heights = [...top.entries()].map(([id, y]) => y + slots.get(id)!.h);
  const result: TreeLayout = {
    nodes,
    edges,
    width: x - (gaps.length ? gaps[gaps.length - 1] : 0) + PAD,
    height: Math.max(...heights, 0) + PAD,
    layers: layers.map((l) => l.filter((id) => !slots.get(id)!.dummy)),
  };
  for (const l of real) {
    if (drawn.has(`${l.from}>${l.to}`)) continue;
    const a = nodes.find((n) => n.id === l.from)!;
    const b = nodes.find((n) => n.id === l.to)!;
    const pts: [number, number][] = [
      [a.x + a.w, a.y + a.anchor],
      [a.x + a.w + 20, a.y + a.anchor],
      [a.x + a.w + 20, Math.max(a.y + a.h, b.y + b.h) + 18],
      [b.x - 20, Math.max(a.y + a.h, b.y + b.h) + 18],
      [b.x - 20, b.y + b.anchor],
      [b.x, b.y + b.anchor],
    ];
    result.edges.push({ from: l.from, to: l.to, d: roundedPath(pts), points: pts });
    result.height = Math.max(result.height, Math.max(a.y + a.h, b.y + b.h) + 18 + PAD);
  }
  return result;
}

/** How many pairs of edges between neighbouring layers cross, for the order in `layers`. */
function crossings(layers: string[][], down: Map<string, string[]>): number {
  let total = 0;
  const pos = new Map<string, number>();
  layers.forEach((l) => l.forEach((id, i) => pos.set(id, i)));
  for (let k = 0; k + 1 < layers.length; k++) {
    const pairs: [number, number][] = [];
    for (const a of layers[k]) for (const b of down.get(a)!) pairs.push([pos.get(a)!, pos.get(b)!]);
    for (let i = 0; i < pairs.length; i++) {
      for (let j = i + 1; j < pairs.length; j++) {
        const [a1, b1] = pairs[i];
        const [a2, b2] = pairs[j];
        if ((a1 - a2) * (b1 - b2) < 0) total++;
      }
    }
  }
  return total;
}

/** A path through `points` (right angles) with the corners rounded. */
export function roundedPath(points: [number, number][]): string {
  // Points on a straight run are not corners.
  const pts = points.filter((p, i) => {
    const a = points[i - 1];
    const b = points[i + 1];
    if (!a || !b) return true;
    return !((a[0] === p[0] && p[0] === b[0]) || (a[1] === p[1] && p[1] === b[1]));
  });
  if (pts.length === 0) return '';
  let d = `M${pts[0][0]},${pts[0][1]}`;
  for (let i = 1; i < pts.length; i++) {
    const p = pts[i];
    const next = pts[i + 1];
    if (!next) {
      d += ` L${p[0]},${p[1]}`;
      break;
    }
    const prev = pts[i - 1];
    const r = Math.min(CORNER, dist(prev, p) / 2, dist(p, next) / 2);
    const a = towards(p, prev, r);
    const b = towards(p, next, r);
    d += ` L${a[0]},${a[1]} Q${p[0]},${p[1]} ${b[0]},${b[1]}`;
  }
  return d;
}

const dist = (a: [number, number], b: [number, number]) => Math.hypot(a[0] - b[0], a[1] - b[1]);

function towards(from: [number, number], to: [number, number], by: number): [number, number] {
  const d = dist(from, to) || 1;
  return [from[0] + ((to[0] - from[0]) / d) * by, from[1] + ((to[1] - from[1]) / d) * by];
}
