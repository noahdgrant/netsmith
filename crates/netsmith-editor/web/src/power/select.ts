// Selecting in the power tree: the path from the sources to a node, and the keys that move along
// the tree.

import type { Link, TreeLayout } from './layout';

export interface Path {
  nodes: Set<string>;
  /** `from>to` of every link on the path. */
  links: Set<string>;
}

export const linkKey = (l: Link): string => `${l.from}>${l.to}`;

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

export type Move = 'up' | 'down' | 'left' | 'right';

/** The node an arrow key goes to from `id`: left to what feeds it, right to what it feeds, up and
 * down to the neighbours in its layer. Null when there is none. With nothing selected, any key
 * picks the first node of the first layer. */
export function move(layout: TreeLayout, links: Link[], id: string | null, key: Move): string | null {
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
  return nearest(ends);
}
