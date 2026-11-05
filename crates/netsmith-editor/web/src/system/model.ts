// The system view as data: a card for each member (a board of a system, or a module, part or
// port of a board) with a table of its ports, and one wire for all the connections between two
// members, left to right from what supplies to what is supplied. Pure functions of what the editor
// sends (`SystemView`, and the power tree of a board for who supplies a net); nothing here draws.

import type { Edge, Location, PowerTree, SystemNode, SystemView } from '../api';
import { amps, rangeText, voltsText } from '../power/format';

/** More ports than this on a card are folded into "N ports". */
export const SHOWN = 6;

export interface PortRow {
  name: string;
  /** `3.3 V  0 A .. 25 mA`: what the port is, short enough for a column. */
  value: string;
  /** The two halves of `value`: the nominal voltage, and the range of the current. */
  volts: string;
  current: string;
  /** The editor's own words for the port, for the tooltip. */
  title: string;
  /** The port supplies (its current is negative). */
  supply: boolean;
}

export interface ModuleModel {
  id: string;
  name: string;
  type: string;
  kind: string;
  at: Location | null;
  ports: PortRow[];
  /** How many wires end on it. */
  wires: number;
}

export interface Wire {
  from: string;
  to: string;
  /** Every connection the wire stands for (net names, or `a.port ~ b.port`), sorted, each once. */
  labels: string[];
  /** What the wire says: the net's name, or `4 nets`. */
  text: string;
  /** `power`: from what supplies to what is supplied; `signal`: no direction is known. */
  kind: 'power' | 'signal';
}

export interface SystemModel {
  modules: ModuleModel[];
  wires: Wire[];
  links: { from: string; to: string }[];
}

const PREFIX: Record<string, number> = { p: 1e-12, n: 1e-9, u: 1e-6, m: 1e-3, k: 1e3, M: 1e6 };

/** The numbers of a quantity's text (`3.234V..3.333V`, `-140.1mA..0A`), in base units. */
export function parseQuantity(text: string | null): [number, number] | null {
  if (!text) return null;
  const found = [...text.matchAll(/(-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?)\s*([pnumkM]?)[AV]/g)].map((m) => Number(m[1]) * (PREFIX[m[2]] ?? 1));
  if (found.length === 0) return null;
  return [Math.min(...found), Math.max(...found)];
}

/** One row of the ports table. */
export function portRow(p: SystemNode['ports'][number]): PortRow {
  const v = parseQuantity(p.voltage);
  const i = parseQuantity(p.current);
  const volts = v ? voltsText((v[0] + v[1]) / 2) : '';
  const current = i ? rangeText(i[0], i[1]) : '';
  const title = [p.name, p.voltage, p.current].filter(Boolean).join('  ');
  return { name: p.name, value: [volts, current].filter(Boolean).join('  '), volts, current, title, supply: !!i && i[1] <= 1e-12 && i[0] < -1e-12 };
}

/** The member a path names (`ldo.vout` is `ldo`). */
const memberOf = (path: string): string => path.split('.')[0];

/** The words of an edge's label that name the two ends of a join (`a.out ~ b.in`), if it is one. */
function joinEnds(label: string): [string, string] | null {
  const parts = label.split(' ~ ');
  return parts.length === 2 ? [parts[0], parts[1]] : null;
}

interface Directed {
  from: string;
  to: string;
  label: string;
  power: boolean;
}

/** Which end supplies, for every edge: from the power tree (who supplies a net) or from the sign
 * of the current on the ports a join names. */
function orient(view: SystemView, power: PowerTree | null): Directed[] {
  const supplied = new Map<string, Set<string>>();
  for (const n of power?.nodes ?? []) supplied.set(n.net, new Set(n.supplies.map((d) => memberOf(d.who))));
  const nodes = new Map(view.nodes.map((n) => [n.id, n]));
  // A part's rail with a negative current supplies. A board's port is the other way round: its
  // current is what is behind it, so a port that others draw from (positive) is the one that gives
  // the power, and one that is supplied from outside (negative) is the one that takes it.
  const supplies = (id: string, port: string): boolean => {
    const p = nodes.get(id)?.ports.find((q) => q.name === port);
    if (!p) return false;
    if (view.kind !== 'system') return portRow(p).supply;
    const i = parseQuantity(p.current);
    return !!i && i[0] >= -1e-12 && i[1] > 1e-12;
  };
  return view.edges
    .filter((e: Edge) => nodes.has(e.from) && nodes.has(e.to) && e.from !== e.to)
    .map((e) => {
      let aSup = false;
      let bSup = false;
      const ends = joinEnds(e.label);
      if (ends) {
        aSup = supplies(e.from, ends[0].slice(e.from.length + 1));
        bSup = supplies(e.to, ends[1].slice(e.to.length + 1));
      } else {
        const who = supplied.get(e.label);
        aSup = !!who?.has(e.from);
        bSup = !!who?.has(e.to);
      }
      if (aSup && !bSup) return { from: e.from, to: e.to, label: e.label, power: true };
      if (bSup && !aSup) return { from: e.to, to: e.from, label: e.label, power: true };
      return { from: e.from, to: e.to, label: e.label, power: false };
    });
}

/** The longest chain of power wires behind each member (cycles cut where they close). */
function ranksOf(ids: string[], power: { from: string; to: string }[]): Map<string, number> {
  const out = new Map<string, string[]>(ids.map((i) => [i, []]));
  for (const w of power) out.get(w.from)!.push(w.to);
  for (const list of out.values()) list.sort();
  const state = new Map<string, number>();
  const fwd: { from: string; to: string }[] = [];
  const visit = (a: string) => {
    state.set(a, 1);
    for (const b of out.get(a)!) {
      if (state.get(b) === 1) continue;
      fwd.push({ from: a, to: b });
      if (!state.has(b)) visit(b);
    }
    state.set(a, 2);
  };
  for (const i of [...ids].sort()) if (!state.has(i)) visit(i);
  const rank = new Map(ids.map((i) => [i, 0]));
  // Relax until nothing moves: the forward links have no cycle, so this ends.
  for (let n = 0; n <= ids.length; n++) {
    let moved = false;
    for (const w of fwd) {
      if (rank.get(w.to)! < rank.get(w.from)! + 1) {
        rank.set(w.to, rank.get(w.from)! + 1);
        moved = true;
      }
    }
    if (!moved) break;
  }
  return rank;
}

/** The model of a view. `power` is the power tree of the same board (null for a system). */
export function modelOf(view: SystemView, power: PowerTree | null): SystemModel {
  const directed = orient(view, power);
  const ids = view.nodes.map((n) => n.id);
  const rank = ranksOf(ids, directed.filter((d) => d.power));
  // Distinct neighbours: the member with the most is the hub, and the hub goes to the right.
  const neighbours = new Map<string, Set<string>>(ids.map((i) => [i, new Set()]));
  for (const d of directed) {
    neighbours.get(d.from)!.add(d.to);
    neighbours.get(d.to)!.add(d.from);
  }
  const key = (id: string) => [rank.get(id)!, neighbours.get(id)!.size] as const;
  const before = (a: string, b: string) => {
    const [ra, da] = key(a);
    const [rb, db] = key(b);
    return ra !== rb ? ra < rb : da !== db ? da < db : a < b;
  };
  // One wire for each pair of members.
  const groups = new Map<string, { a: string; b: string; items: Directed[] }>();
  for (const d of directed) {
    const [a, b] = d.from < d.to ? [d.from, d.to] : [d.to, d.from];
    const k = `${a}\u0000${b}`;
    (groups.get(k) ?? groups.set(k, { a, b, items: [] }).get(k)!).items.push(d);
  }
  const wires: Wire[] = [...groups.values()]
    .map((g) => {
      const power = g.items.filter((d) => d.power);
      let from: string;
      let to: string;
      if (power.length > 0) {
        const forward = power.filter((d) => d.from === g.a).length;
        [from, to] = forward >= power.length - forward ? [g.a, g.b] : [g.b, g.a];
      } else {
        [from, to] = before(g.a, g.b) ? [g.a, g.b] : [g.b, g.a];
      }
      const labels = [...new Set(g.items.map((d) => d.label))].sort();
      const joins = labels.every((l) => joinEnds(l));
      return {
        from,
        to,
        labels,
        text: labels.length === 1 ? labels[0] : `${labels.length} ${joins ? 'joins' : 'nets'}`,
        kind: power.length > 0 ? ('power' as const) : ('signal' as const),
      };
    })
    .sort((x, y) => x.from.localeCompare(y.from) || x.to.localeCompare(y.to));
  const count = new Map<string, number>(ids.map((i) => [i, 0]));
  for (const w of wires) {
    count.set(w.from, count.get(w.from)! + 1);
    count.set(w.to, count.get(w.to)! + 1);
  }
  const modules: ModuleModel[] = view.nodes.map((n) => ({
    id: n.id,
    name: n.name,
    type: n.typeName,
    kind: n.kind,
    at: n.at,
    ports: n.ports.map(portRow),
    wires: count.get(n.id)!,
  }));
  return { modules, wires, links: wires.map((w) => ({ from: w.from, to: w.to })) };
}

/** The wire's text cut to `max` characters, with an ellipsis. */
export const shorten = (text: string, max: number): string => (text.length <= max ? text : `${text.slice(0, max - 1)}…`);

/** An amount for a tooltip. */
export const amount = (x: number): string => {
  const a = amps(x);
  return `${a.num} ${a.unit}`;
};
