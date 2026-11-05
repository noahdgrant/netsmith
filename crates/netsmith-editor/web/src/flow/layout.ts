// The layout of a flow drawing (the power tree, the system view): a left-to-right hierarchy.
// Nodes sit in layers, every layer to the right of what feeds it. Every edge runs in a free
// channel: between layers (in lanes of the gap), through a lane of its own in a layer it
// passes, and, for an edge that goes back (a cycle), along a band under the whole drawing, so
// no edge runs behind or through any node. Edges are right angles with rounded corners; their
// ends sit on a grid, so a wire is straight unless it must turn and no jog is smaller than a
// step. Nothing is random: ids are sorted, ties are broken by name, and the same graph always
// gives the same drawing.

export interface Item {
  id: string;
  /** What the node is (`rail`, `stage`, `module`, ...): the layout does not read it. */
  kind: string;
  w: number;
  h: number;
  /** Where edges meet the node, from its top: the middle of a stage, the header of a rail. */
  anchor: number;
  /** The space between the ends of the edges on one side of the node (a multiple of the grid). */
  portStep?: number;
}

export interface Link {
  from: string;
  to: string;
  /** A label for the edge and its width in pixels: it sits above the edge's first run. */
  label?: string;
  labelW?: number;
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
  /** The corners of the path, left to right (back edges go round under the drawing). */
  points: [number, number][];
  /** The edge goes back: it closes a cycle. */
  back: boolean;
  /** Where the label goes: the left end and the baseline of its text. */
  label?: { text: string; x: number; y: number; w: number };
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
/** The grid the ends of the edges sit on. */
export const GRID = 8;
/** The least space between two nodes of a layer. */
const SEP = 16;
/** The space between nodes of a layer when there is room. */
export const GAP_Y = 24;
/** The least space between two layers, for the edges between them. */
export const GAP_X = 48;
/** The space one edge's vertical run takes in the gap. */
const LANE = 16;
/** The height of the lane a long edge keeps in a layer it passes. */
const THROUGH = 8;
const CORNER = 7;
const SWEEPS = 6;
const LABEL_PAD = 10;

interface Slot {
  id: string;
  dummy: boolean;
  w: number;
  h: number;
  anchor: number;
  step: number;
}

/** One straight stretch of an edge between two slots (or, for a back edge, to the band). */
interface Seg {
  link: number;
  a: string;
  b: string;
  /** 0 for a slot to the next slot; `out` and `in` for the two halves of a back edge. */
  part: 'fwd' | 'out' | 'in';
  first: boolean;
}

const snap = (v: number) => Math.round(v / GRID) * GRID;

/** Lay out `items` joined by `links`. Links whose ends are not items are left out. */
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
  const isForward = new Set<string>();
  const visit = (a: string) => {
    state.set(a, 1);
    for (const b of out.get(a)!) {
      if (state.get(b) === 1) continue;
      isForward.add(`${a}>${b}`);
      if (!state.has(b)) visit(b);
    }
    state.set(a, 2);
  };
  for (const i of sorted) if (!state.has(i.id)) visit(i.id);
  const layer = new Map<string, number>(sorted.map((i) => [i.id, 0]));
  const incoming = new Map<string, number>(sorted.map((i) => [i.id, 0]));
  const forwardOut = new Map<string, string[]>(sorted.map((i) => [i.id, []]));
  for (const l of real) {
    if (!isForward.has(`${l.from}>${l.to}`)) continue;
    incoming.set(l.to, incoming.get(l.to)! + 1);
    forwardOut.get(l.from)!.push(l.to);
  }
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
  // The ends of labelled edges leaving a node are three steps apart, so two labels never touch.
  const labelled = new Set(real.filter((l) => l.label).map((l) => l.from));
  const slots = new Map<string, Slot>(
    sorted.map((i) => [i.id, { id: i.id, dummy: false, w: i.w, h: i.h, anchor: i.anchor, step: Math.max(i.portStep ?? GRID, labelled.has(i.id) ? 3 * GRID : GRID) }]),
  );
  const layerOf = new Map(layer);
  const chains: { link: number; ids: string[]; back: boolean }[] = [];
  real.forEach((l, n) => {
    if (!isForward.has(`${l.from}>${l.to}`)) {
      chains.push({ link: n, ids: [l.from, l.to], back: true });
      return;
    }
    const ids = [l.from];
    for (let k = layer.get(l.from)! + 1; k < layer.get(l.to)!; k++) {
      const id = `~${l.from}>${l.to}@${k}`;
      slots.set(id, { id, dummy: true, w: THROUGH, h: THROUGH, anchor: THROUGH / 2, step: GRID });
      layerOf.set(id, k);
      ids.push(id);
    }
    ids.push(l.to);
    chains.push({ link: n, ids, back: false });
  });
  const depth = Math.max(-1, ...[...layerOf.values()]);
  const layers: string[][] = Array.from({ length: depth + 1 }, () => []);
  for (const id of [...sorted.map((i) => i.id), ...[...slots.keys()].filter((k) => k.startsWith('~'))]) layers[layerOf.get(id)!].push(id);

  // The links between neighbouring layers (back edges take no part in the order).
  const down = new Map<string, string[]>();
  const up = new Map<string, string[]>();
  for (const id of slots.keys()) {
    down.set(id, []);
    up.set(id, []);
  }
  for (const c of chains) {
    if (c.back) continue;
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
  // The ends of the edges sit on the grid: the anchor of each node is on a line of it.
  for (const l of layers) {
    let min = -Infinity;
    for (const id of l) {
      const s = slots.get(id)!;
      const t = Math.max(snap(top.get(id)! + s.anchor) - s.anchor, min);
      top.set(id, t);
      min = t + s.h + (s.dummy ? 8 : SEP);
    }
  }

  // The segments of the edges, and the ends of each on its nodes.
  const segs: Seg[] = [];
  chains.forEach((c) => {
    if (c.back) {
      segs.push({ link: c.link, a: c.ids[0], b: c.ids[1], part: 'out', first: true }, { link: c.link, a: c.ids[0], b: c.ids[1], part: 'in', first: false });
      return;
    }
    for (let k = 0; k + 1 < c.ids.length; k++) segs.push({ link: c.link, a: c.ids[k], b: c.ids[k + 1], part: 'fwd', first: k === 0 });
  });
  const segKey = (s: Seg) => `${s.link}:${s.part}:${s.a}>${s.b}`;
  const centre = (id: string) => top.get(id)! + slots.get(id)!.anchor;
  /** The y of the end of segment `s` on slot `id` (`a` leaves it, `b` enters it). */
  const ends = () => {
    const outs = new Map<string, Seg[]>();
    const ins = new Map<string, Seg[]>();
    for (const s of segs) {
      if (s.part === 'in') {
        (ins.get(s.b) ?? ins.set(s.b, []).get(s.b)!).push(s);
      } else if (s.part === 'out') {
        (outs.get(s.a) ?? outs.set(s.a, []).get(s.a)!).push(s);
      } else {
        (outs.get(s.a) ?? outs.set(s.a, []).get(s.a)!).push(s);
        (ins.get(s.b) ?? ins.set(s.b, []).get(s.b)!).push(s);
      }
    }
    const y = new Map<string, number>();
    const spread = (id: string, list: Seg[], side: 'out' | 'in') => {
      const slot = slots.get(id)!;
      // Back edges take the last ends, so they never cross the others on the node.
      const key = (s: Seg) => (s.part === 'fwd' ? centre(side === 'out' ? s.b : s.a) : Infinity);
      const sorted2 = [...list].sort((p, q) => key(p) - key(q) || segKey(p).localeCompare(segKey(q)));
      const n = sorted2.length;
      const step = slot.dummy ? 0 : slot.step;
      let first = -Math.floor((n - 1) / 2) * step;
      // The ends stay inside the node.
      const lo = GRID - slot.anchor;
      const hi = slot.h - GRID - slot.anchor;
      if (!slot.dummy) {
        if (first < lo) first += Math.ceil((lo - first) / GRID) * GRID;
        const last = first + (n - 1) * step;
        if (last > hi) first -= Math.ceil((last - hi) / GRID) * GRID;
      }
      sorted2.forEach((s, i) => y.set(`${segKey(s)}|${side}`, top.get(id)! + slot.anchor + first + i * step));
    };
    for (const [id, list] of outs) spread(id, list, 'out');
    for (const [id, list] of ins) spread(id, list, 'in');
    return y;
  };

  // Straightening: a jog between the two ends of an edge is moved out by shifting a node a
  // step or two, where the neighbours in its layer leave room.
  const cost = (y: Map<string, number>, incident: Set<Seg>) => {
    let c = 0;
    for (const s of incident) {
      if (s.part !== 'fwd') continue;
      const d = Math.abs(y.get(`${segKey(s)}|out`)! - y.get(`${segKey(s)}|in`)!);
      c += d < 0.5 ? 0 : d <= 3 * GRID ? 3 : 1;
    }
    return c;
  };
  const incidentOf = new Map<string, Set<Seg>>();
  for (const s of segs) {
    for (const id of [s.a, s.b]) (incidentOf.get(id) ?? incidentOf.set(id, new Set()).get(id)!).add(s);
  }
  for (let pass = 0; pass < 4; pass++) {
    let moved = false;
    for (const l of layers) {
      l.forEach((id, i) => {
        const inc = incidentOf.get(id);
        if (!inc || inc.size === 0) return;
        const slot = slots.get(id)!;
        const before = cost(ends(), inc);
        if (before === 0) return;
        const prev = i > 0 ? top.get(l[i - 1])! + slots.get(l[i - 1])!.h + (slots.get(l[i - 1])!.dummy || slot.dummy ? 8 : SEP) : -Infinity;
        const next = i + 1 < l.length ? top.get(l[i + 1])! - slot.h - (slots.get(l[i + 1])!.dummy || slot.dummy ? 8 : SEP) : Infinity;
        const here = top.get(id)!;
        let bestT = here;
        let bestC = before;
        for (const d of [-3, -2, -1, 1, 2, 3].map((k) => k * GRID)) {
          const t = here + d;
          if (t < PAD - 0.1 && !slot.dummy && i === 0) continue;
          if (t < prev || t > next) continue;
          top.set(id, t);
          const c = cost(ends(), inc);
          if (c < bestC || (c === bestC && c < before && Math.abs(d) < Math.abs(bestT - here))) {
            bestC = c;
            bestT = t;
          }
        }
        top.set(id, bestT);
        if (bestT !== here) moved = true;
      });
    }
    if (!moved) break;
  }
  const minTop = Math.min(...[...top.entries()].filter(([id]) => !slots.get(id)!.dummy).map(([, t]) => t), PAD);
  if (minTop < PAD) {
    const lift = Math.ceil((PAD - minTop) / GRID) * GRID;
    for (const id of top.keys()) top.set(id, top.get(id)! + lift);
  }
  const portY = ends();

  // The bottom of the drawing, under which the edges that go back run.
  const bottom = Math.max(...[...top.entries()].map(([id, y]) => y + slots.get(id)!.h), 0);
  const backs = chains.filter((c) => c.back);

  // The gaps: gap g is the space left of layer g (0 is the margin, `layers.length` the one on
  // the right). Each segment that changes height takes a lane of its own in a gap.
  const gapCount = layers.length + 1;
  interface Run {
    s: Seg;
    gap: number;
    ya: number;
    yb: number;
    /** From the left of the gap to the lane: room for a label. */
    base: number;
    lane: number;
  }
  const bandY = (n: number) => snap(bottom + 3 * GRID + n * LANE);
  const runs: Run[] = [];
  const labelOf = (link: number) => (real[link].label && real[link].labelW ? real[link].labelW! + LABEL_PAD : 0);
  backs.forEach((c, n) => {
    const outs = segs.find((s) => s.link === c.link && s.part === 'out')!;
    const ins = segs.find((s) => s.link === c.link && s.part === 'in')!;
    runs.push({ s: outs, gap: layerOf.get(c.ids[0])! + 1, ya: portY.get(`${segKey(outs)}|out`)!, yb: bandY(n), base: labelOf(c.link), lane: 0 });
    runs.push({ s: ins, gap: layerOf.get(c.ids[1])!, ya: bandY(n), yb: portY.get(`${segKey(ins)}|in`)!, base: 0, lane: 0 });
  });
  for (const s of segs) {
    if (s.part !== 'fwd') continue;
    runs.push({ s, gap: layerOf.get(s.b)!, ya: portY.get(`${segKey(s)}|out`)!, yb: portY.get(`${segKey(s)}|in`)!, base: s.first ? labelOf(s.link) : 0, lane: 0 });
  }
  const lanesIn: number[] = new Array(gapCount).fill(0);
  const needed: number[] = new Array(gapCount).fill(0);
  for (let g = 0; g < gapCount; g++) {
    const here = runs.filter((r) => r.gap === g);
    const moving = here.filter((r) => Math.abs(r.ya - r.yb) > 0.5).sort((p, q) => Math.min(p.ya, p.yb) - Math.min(q.ya, q.yb) || segKey(p.s).localeCompare(segKey(q.s)));
    const endsAt: number[] = [];
    for (const r of moving) {
      const lo = Math.min(r.ya, r.yb);
      const hi = Math.max(r.ya, r.yb);
      let k = endsAt.findIndex((e) => e < lo - 4);
      if (k < 0) k = endsAt.length;
      endsAt[k] = hi;
      r.lane = k;
    }
    lanesIn[g] = endsAt.length;
    for (const r of here) needed[g] = Math.max(needed[g], r.base);
  }

  // Horizontal places: a layer is as wide as its widest node; a gap as wide as its lanes and
  // the labels that stand in it.
  const width = layers.map((l) => Math.max(0, ...l.map((id) => slots.get(id)!.w)));
  const gapW = layers.length
    ? Array.from({ length: gapCount }, (_, g) => {
        const lanes = lanesIn[g];
        const room = Math.max(needed[g], LANE) + lanes * LANE;
        if (g === 0 || g === layers.length) return lanes === 0 && needed[g] === 0 ? 0 : room + LANE;
        return Math.max(GAP_X, room + LANE);
      })
    : [];
  const gapLeft: number[] = [];
  const left: number[] = [];
  let x = PAD;
  for (let g = 0; g < gapCount; g++) {
    gapLeft.push(x);
    x += gapW[g] ?? 0;
    if (g < layers.length) {
      left.push(x);
      x += width[g];
    }
  }
  const laneX = (r: Run) => gapLeft[r.gap] + Math.max(LANE, r.base) + r.lane * LANE;

  const nodes: Placed[] = [];
  layers.forEach((l, k) => {
    let order = 0;
    for (const id of l) {
      if (slots.get(id)!.dummy) continue;
      nodes.push({ ...byId.get(id)!, x: left[k], y: top.get(id)!, layer: k, order: order++ });
    }
  });

  const runOf = new Map(runs.map((r) => [segKey(r.s), r]));
  const edges: Routed[] = chains.map((c) => {
    const l = real[c.link];
    const pts: [number, number][] = [];
    let label: Routed['label'];
    const push = (px: number, py: number) => {
      const last = pts[pts.length - 1];
      if (!last || last[0] !== px || last[1] !== py) pts.push([px, py]);
    };
    if (c.back) {
      const outs = segs.find((s) => s.link === c.link && s.part === 'out')!;
      const ins = segs.find((s) => s.link === c.link && s.part === 'in')!;
      const r1 = runOf.get(segKey(outs))!;
      const r2 = runOf.get(segKey(ins))!;
      const a = slots.get(c.ids[0])!;
      const x0 = left[layerOf.get(c.ids[0])!] + a.w;
      push(x0, r1.ya);
      push(laneX(r1), r1.ya);
      push(laneX(r1), r1.yb);
      push(laneX(r2), r2.ya);
      push(laneX(r2), r2.yb);
      push(left[layerOf.get(c.ids[1])!], r2.yb);
      if (l.label && l.labelW) label = { text: l.label, x: x0 + 5, y: r1.ya - 4, w: l.labelW };
    } else {
      for (let k = 0; k + 1 < c.ids.length; k++) {
        const a = c.ids[k];
        const b = c.ids[k + 1];
        const s = segs.find((q) => q.link === c.link && q.part === 'fwd' && q.a === a && q.b === b)!;
        const r = runOf.get(segKey(s))!;
        const la = layerOf.get(a)!;
        const xa = left[la] + slots.get(a)!.w;
        const xb = left[la + 1];
        // A jog the grid cannot explain is drawn straight.
        const yb = Math.abs(r.ya - r.yb) < GRID / 2 ? r.ya : r.yb;
        push(xa, r.ya);
        if (Math.abs(r.ya - yb) > 0.5) {
          const lx = laneX(r);
          push(lx, r.ya);
          push(lx, yb);
        }
        push(xb, yb);
        if (slots.get(b)!.dummy) push(xb + THROUGH, yb);
        if (k === 0 && l.label && l.labelW) label = { text: l.label, x: xa + 5, y: r.ya - 4, w: l.labelW };
      }
    }
    return { from: l.from, to: l.to, d: roundedPath(pts), points: pts, back: c.back, label };
  });

  const lastGap = gapW.length ? gapW[gapW.length - 1] : 0;
  return {
    nodes,
    edges,
    width: Math.max(x + lastGap * 0 + PAD, 2 * PAD),
    height: Math.max(bottom + (backs.length ? 3 * GRID + backs.length * LANE : 0), 0) + PAD,
    layers: layers.map((l) => l.filter((id) => !slots.get(id)!.dummy)),
  };
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
