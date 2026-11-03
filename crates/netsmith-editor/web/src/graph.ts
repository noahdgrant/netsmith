// Layered layout of a small directed graph, left to right: a node sits one column to the
// right of the furthest node that feeds it.

export interface Box {
  id: string;
  w: number;
  h: number;
}

export interface Link {
  from: string;
  to: string;
}

export interface Placed extends Box {
  x: number;
  y: number;
  column: number;
}

export interface Layout {
  nodes: Placed[];
  width: number;
  height: number;
}

const GAP_X = 92;
const GAP_Y = 28;
const PAD = 40;
/** More nodes than this in one column and the column may be cut into lanes. */
const MANY = 6;
/** The width over the height the lanes aim for. */
const ASPECT = 1.6;

export function layoutGraph(boxes: Box[], links: Link[], gapX = GAP_X): Layout {
  const index = new Map(boxes.map((b, i) => [b.id, i]));
  const real = links.filter((l) => index.has(l.from) && index.has(l.to) && l.from !== l.to);
  // Edges that close a cycle are left out of the depth, so a cycle still lays out.
  const outgoing = boxes.map(() => [] as number[]);
  for (const l of real) outgoing[index.get(l.from)!].push(index.get(l.to)!);
  const state = boxes.map(() => 0); // 0 new, 1 on the path, 2 done
  const forward = outgoing.map(() => [] as number[]);
  const visit = (a: number) => {
    state[a] = 1;
    for (const b of outgoing[a]) {
      if (state[b] === 1) continue;
      forward[a].push(b);
      if (state[b] === 0) visit(b);
    }
    state[a] = 2;
  };
  boxes.forEach((_, a) => state[a] === 0 && visit(a));
  // Longest path from a source.
  const depth = boxes.map(() => 0);
  const incoming = boxes.map(() => 0);
  forward.forEach((targets) => targets.forEach((b) => incoming[b]++));
  const queue = incoming.flatMap((n, i) => (n === 0 ? [i] : []));
  while (queue.length) {
    const a = queue.shift()!;
    for (const b of forward[a]) {
      depth[b] = Math.max(depth[b], depth[a] + 1);
      if (--incoming[b] === 0) queue.push(b);
    }
  }
  const byDepth: number[][] = [];
  boxes.forEach((_, i) => (byDepth[depth[i]] ??= []).push(i));
  // A column with many nodes (every rail nothing feeds is in the first) would be a tall
  // strip: it is cut into lanes, side by side, so the whole is about as wide as it is tall.
  const area = boxes.reduce((sum, b) => sum + (b.w + gapX) * (b.h + GAP_Y), 0);
  const limit = Math.max(Math.max(0, ...boxes.map((b) => b.h)) * 2, Math.sqrt(area / ASPECT));
  const columns: { depth: number; nodes: number[] }[] = [];
  byDepth.forEach((c, d) => {
    if (!c) return;
    const total = c.reduce((s, i) => s + boxes[i].h + GAP_Y, 0) - GAP_Y;
    if (c.length <= MANY || total <= limit) {
      columns.push({ depth: d, nodes: c });
      return;
    }
    let lane: number[] = [];
    let height = 0;
    for (const i of c) {
      if (lane.length && height + GAP_Y + boxes[i].h > limit) {
        columns.push({ depth: d, nodes: lane });
        lane = [];
        height = 0;
      }
      height += (lane.length ? GAP_Y : 0) + boxes[i].h;
      lane.push(i);
    }
    columns.push({ depth: d, nodes: lane });
  });
  const widths = columns.map((c) => Math.max(0, ...c.nodes.map((i) => boxes[i].w)));
  const heights = columns.map((c) => c.nodes.reduce((s, i) => s + boxes[i].h, 0) + Math.max(0, c.nodes.length - 1) * GAP_Y);
  const tallest = Math.max(0, ...heights);
  const placed: Placed[] = new Array(boxes.length);
  let x = PAD;
  columns.forEach((c, k) => {
    let y = PAD + (tallest - heights[k]) / 2;
    for (const i of c.nodes) {
      placed[i] = { ...boxes[i], x, y, column: c.depth };
      y += boxes[i].h + GAP_Y;
    }
    x += widths[k] + gapX;
  });
  return {
    nodes: placed.filter(Boolean),
    width: Math.max(x - gapX + PAD, 2 * PAD),
    height: tallest + 2 * PAD,
  };
}

/** The path of a curve from the right side of one node to the left side of another. */
export function curve(a: Placed, b: Placed): { d: string; mx: number; my: number } {
  const x1 = a.x + a.w;
  const y1 = a.y + a.h / 2;
  const x2 = b.x;
  const y2 = b.y + b.h / 2;
  const dx = Math.max(40, Math.abs(x2 - x1) / 2);
  return {
    d: `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`,
    mx: (x1 + x2) / 2,
    my: (y1 + y2) / 2,
  };
}
