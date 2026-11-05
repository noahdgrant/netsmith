// Selecting in a flow drawing (the power tree, the system view): what feeds a node, what it
// feeds, and the keys that move along the drawing.

import type { Link, TreeLayout } from './layout';

export interface Path {
  nodes: Set<string>;
  /** `from>to` of every link on the path. */
  links: Set<string>;
}

export const linkKey = (l: { from: string; to: string }): string => `${l.from}>${l.to}`;

/** Every node that feeds `id`, through any number of stages, and the links between them. */
export function upstream(links: Link[], id: string): Path {
  const into = new Map<string, Link[]>();
  for (const l of links) into.set(l.to, [...(into.get(l.to) ?? []), l]);
  const nodes = new Set([id]);
  const used = new Set<string>();
  const queue = [id];
  while (queue.length) {
    const n = queue.shift()!;
    for (const l of into.get(n) ?? []) {
      used.add(linkKey(l));
      if (!nodes.has(l.from)) {
        nodes.add(l.from);
        queue.push(l.from);
      }
    }
  }
  return { nodes, links: used };
}

/** Every node `id` feeds, through any number of nodes, and the links between them. */
export function downstream(links: Link[], id: string): Path {
  return upstream(links.map((l) => ({ from: l.to, to: l.from })), id);
}

/** What a selected node lights: what feeds it (`up`) and what it feeds (`down`). A node with
 * nothing before it lights what it feeds; a node with nothing after it, what feeds it; a node
 * between lights both, in two tints. */
export interface Trace {
  up: Path;
  down: Path;
  /** Every node of either, the selected one included. */
  nodes: Set<string>;
}

export function trace(links: Link[], id: string): Trace {
  const up = upstream(links, id);
  const down = downstream(links, id);
  // The links of `down` were read backwards: put them the right way round.
  const flipped = new Set([...down.links].map((k) => k.split('>').reverse().join('>')));
  return { up, down: { nodes: down.nodes, links: flipped }, nodes: new Set([...up.nodes, ...down.nodes]) };
}

export type Move = 'up' | 'down' | 'left' | 'right';

/** The node an arrow key goes to from `id`: left to what feeds it, right to what it feeds, up and
 * down to the neighbours in its layer; `prefer` is taken when it is one of the ends of a left or
 * right move. Null when there is none. With nothing selected, any key
 * picks the first node of the first layer. */
export function move(layout: TreeLayout, links: Link[], id: string | null, key: Move, prefer: string | null = null): string | null {
  const first = layout.layers.find((l) => l.length)?.[0] ?? null;
  if (id === null) return first;
  const here = layout.nodes.find((n) => n.id === id);
  if (!here) return first;
  const place = new Map(layout.nodes.map((n) => [n.id, n]));
  const nearest = (ids: string[]) =>
    [...ids].sort(
      (a, b) =>
        Math.abs(place.get(a)!.y + place.get(a)!.anchor - (here.y + here.anchor)) -
          Math.abs(place.get(b)!.y + place.get(b)!.anchor - (here.y + here.anchor)) || a.localeCompare(b),
    )[0] ?? null;
  if (key === 'up' || key === 'down') {
    const layer = layout.layers[here.layer];
    const at = layer.indexOf(id) + (key === 'up' ? -1 : 1);
    return layer[at] ?? null;
  }
  // Left and right follow the links, which reach past a layer when a rail feeds a far stage.
  const ends = links
    .filter((l) => (key === 'left' ? l.to === id : l.from === id))
    .map((l) => (key === 'left' ? l.from : l.to))
    .filter((n) => place.has(n));
  // Going back the way one came is the likeliest wish: Left then Right ends where it began.
  return prefer && ends.includes(prefer) ? prefer : nearest(ends);
}
