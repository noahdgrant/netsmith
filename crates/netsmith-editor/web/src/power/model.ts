// The power tree of the page, as data: what each rail card says, what each converter is, which
// rails have trouble, and in what order the loads are listed. Pure functions of what the editor
// sends (`PowerTree`); nothing here draws.

import type { Edge, Location, PowerIssue, PowerNode, PowerTree, Range, Stage } from '../api';
import { NEGLIGIBLE, percentText, voltsText } from './format';

/** The heaviest loads a card lists before "+N more". */
export const TOP = 5;

export interface Load {
  /** The path the editor gives: `mcu.vdd`. */
  who: string;
  /** What is before the last dot: the part (or module) the pin is on. */
  part: string;
  /** What is after it: the pin or rail of the part. Empty when `who` has no dot. */
  pin: string;
  /** `rail` for a rail that was declared, `chain` for one the solver made of a part's pin. */
  kind: string;
  at: Location | null;
  lo: number;
  hi: number;
  /** The current is not settled. */
  unknown: boolean;
  /** It is settled and nothing: a reference pin, a pin that draws nothing. */
  zero: boolean;
  /** The biggest current it can be, either way. */
  weight: number;
}

export type StageKind = 'buck' | 'boost' | 'ldo' | 'protection' | 'filter' | 'switch' | 'diode' | 'link' | 'converter';

export interface StageModel {
  id: string;
  /** The instance's path: `ldo`. */
  name: string;
  /** Its type: `LdoStage`. */
  type: string;
  kind: StageKind;
  /** `LDO 3.3 V`. */
  title: string;
  from: string[];
  to: string[];
  at: Location | null;
  /** `86 % .. 94 %`, when it has one. */
  efficiency: string | null;
  /** It has an efficiency and the solver did not settle it. */
  efficiencyUnknown: boolean;
}

export type Level = 'bad' | 'warn';

/** What the server says is wrong with a rail or a converter (`PowerTree.issues`); the page only draws it. */
export interface Issue {
  /** The node the issue is on. */
  id: string;
  level: Level;
  /** `V3V3`. */
  where: string;
  text: string;
  /** The place in the source to look at, as the server says. */
  at: Location | null;
}

export interface RailModel {
  id: string;
  net: string;
  board: string | null;
  voltage: Range | null;
  /** What the rail is supplied by (negative currents): sources and converters' outputs. */
  supplies: Load[];
  /** Every load, the converters that this rail feeds left out, heaviest first. */
  loads: Load[];
  /** The first `TOP` loads that draw something. */
  top: Load[];
  /** The loads that draw something and are not in `top`. */
  more: Load[];
  /** The loads that are settled at 0 A. */
  zeros: Load[];
  /** The loads with a current the solver has not settled. */
  unknowns: Load[];
  /** The converters this rail feeds: their names. */
  feeds: string[];
  demand: Range | null;
  capacity: Range | null;
  capacityFrom: string | null;
  budget: PowerNode['budget'];
  /** The demand's top over the capacity's, when both are known. */
  fill: number | null;
  /** What is left of the capacity at the worst case, in amperes; negative when over. */
  headroom: number | null;
  /** Whether anything supplies it: a source, or a converter. */
  sourced: boolean;
  /** Where to look when it has an issue (`PowerNode.at`). */
  at: Location | null;
}

export interface PowerModel {
  rails: RailModel[];
  stages: StageModel[];
  issues: Issue[];
  /** The links of the drawing: rail to stage, stage to rail. */
  links: { from: string; to: string }[];
}

const isZero = (lo: number, hi: number) => Math.abs(lo) <= NEGLIGIBLE && Math.abs(hi) <= NEGLIGIBLE;

/** The load a draw of the editor is. */
export function loadOf(d: { who: string; kind: string; at: Location | null; range: Range | null }): Load {
  const dot = d.who.lastIndexOf('.');
  const lo = d.range?.lo ?? 0;
  const hi = d.range?.hi ?? 0;
  return {
    who: d.who,
    part: dot < 0 ? d.who : d.who.slice(0, dot),
    pin: dot < 0 ? '' : d.who.slice(dot + 1),
    kind: d.kind,
    at: d.at,
    lo,
    hi,
    unknown: d.range === null,
    zero: d.range !== null && isZero(lo, hi),
    weight: Math.max(Math.abs(lo), Math.abs(hi)),
  };
}

/** Heaviest first; equal currents by name, so the order never changes between reads. */
export function byWeight(a: Load, b: Load): number {
  return b.weight - a.weight || a.who.localeCompare(b.who);
}

/** The name and the type in an edge's label (`ldo (LdoStage)`). */
export function splitLabel(label: string): { name: string; type: string } {
  const open = label.indexOf(' (');
  if (open < 0 || !label.endsWith(')')) return { name: label, type: '' };
  return { name: label.slice(0, open), type: label.slice(open + 2, -1) };
}

/** What kind of stage a converter is, from its type's name, else from what it does to the
 * voltage. A join of two boards is a `link`. */
export function stageKind(type: string, edge: 'converts' | 'join' | 'net', vin: Range | null, vout: Range | null): StageKind {
  if (edge !== 'converts') return 'link';
  const t = type.toLowerCase();
  if (/buck|step.?down/.test(t)) return 'buck';
  if (/boost|step.?up/.test(t)) return 'boost';
  if (/ldo|linear/.test(t)) return 'ldo';
  if (/protect|surge|tvs|fuse|polarity|clamp/.test(t)) return 'protection';
  if (/filter|ferrite|bead/.test(t)) return 'filter';
  if (/switch|load.?sw/.test(t)) return 'switch';
  if (/diode|oring|or.?ing/.test(t)) return 'diode';
  if (vin && vout) {
    if (vout.hi < vin.lo) return 'buck';
    if (vout.lo > vin.hi) return 'boost';
  }
  return 'converter';
}

const KIND_NAME: Record<StageKind, string> = {
  buck: 'Buck',
  boost: 'Boost',
  ldo: 'LDO',
  protection: 'Protection',
  filter: 'Filter',
  switch: 'Load switch',
  diode: 'Diode',
  link: 'Board link',
  converter: 'Converter',
};

export const kindName = (k: StageKind): string => KIND_NAME[k];

/** The middle of a range of voltages. */
const nominal = (r: Range) => (r.lo + r.hi) / 2;

function stageTitle(kind: StageKind, out: Range | null): string {
  // The regulators and converters say what they make; a stage that only passes a voltage on does not.
  const makes = kind === 'buck' || kind === 'boost' || kind === 'ldo' || kind === 'converter';
  return makes && out ? `${KIND_NAME[kind]} ${voltsText(nominal(out))}` : KIND_NAME[kind];
}

/** The model of a tree. */
export function modelOf(tree: PowerTree): PowerModel {
  const byId = new Map(tree.nodes.map((n) => [n.id, n]));
  const stageInfo = new Map<string, Stage>((tree.stages ?? []).map((s) => [s.label, s]));
  // One stage for each label: a converter with two outputs is one box.
  const stages = new Map<string, StageModel>();
  for (const e of sortedEdges(tree.edges)) {
    if (!byId.has(e.from) || !byId.has(e.to)) continue;
    const id = `stage:${e.label}`;
    let s = stages.get(id);
    if (!s) {
      const { name, type } = splitLabel(e.label);
      const info = stageInfo.get(e.label);
      s = {
        id,
        name,
        type,
        kind: 'converter',
        title: '',
        from: [],
        to: [],
        at: info?.at ?? e.at,
        efficiency: info?.efficiency ? rangeFractions(info.efficiency) : null,
        efficiencyUnknown: info?.efficiencyUnsettled ?? false,
      };
      stages.set(id, s);
    }
    if (!s.from.includes(e.from)) s.from.push(e.from);
    if (!s.to.includes(e.to)) s.to.push(e.to);
    const vin = byId.get(e.from)?.voltage ?? null;
    const vout = byId.get(e.to)?.voltage ?? null;
    s.kind = stageKind(s.type, e.kind, vin, vout);
    s.title = stageTitle(s.kind, vout);
  }
  const stageList = [...stages.values()].sort((a, b) => a.id.localeCompare(b.id));
  const fedBy = new Map<string, StageModel[]>();
  for (const s of stageList) for (const r of s.from) fedBy.set(r, [...(fedBy.get(r) ?? []), s]);
  const supplied = new Set(stageList.flatMap((s) => s.to));

  const rails: RailModel[] = [...tree.nodes]
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((n) => railOf(n, fedBy.get(n.id) ?? [], supplied.has(n.id)));

  const links = stageList.flatMap((s) => [
    ...s.from.map((r) => ({ from: r, to: s.id })),
    ...s.to.map((r) => ({ from: s.id, to: r })),
  ]);
  return { rails, stages: stageList, issues: (tree.issues ?? []).map(issueOf), links };
}

function sortedEdges(edges: Edge[]): Edge[] {
  return [...edges].sort((a, b) => a.label.localeCompare(b.label) || a.from.localeCompare(b.from) || a.to.localeCompare(b.to));
}

/** `0.86..0.94` as `86 % .. 94 %`. */
function rangeFractions(r: Range): string {
  const a = percentText(r.lo);
  const b = percentText(r.hi);
  return a === b ? a : `${a} .. ${b}`;
}

function railOf(n: PowerNode, feeding: StageModel[], supplied: boolean): RailModel {
  const feedNames = new Set(feeding.map((s) => s.name));
  const isFeed = (who: string) => {
    const dot = who.lastIndexOf('.');
    return dot > 0 && feedNames.has(who.slice(0, dot));
  };
  const supplies = n.supplies.map(loadOf).sort(byWeight);
  const loads = n.loads.filter((d) => !isFeed(d.who)).map(loadOf).sort(byWeight);
  const drawing = loads.filter((l) => !l.zero && !l.unknown);
  const fill = n.demand && n.capacity && n.capacity.hi > 0 ? n.demand.hi / n.capacity.hi : null;
  return {
    id: n.id,
    net: n.net,
    board: n.board,
    voltage: n.voltage,
    supplies,
    loads,
    top: drawing.slice(0, TOP),
    more: drawing.slice(TOP),
    zeros: loads.filter((l) => l.zero),
    unknowns: loads.filter((l) => l.unknown),
    feeds: feeding.map((s) => s.name),
    demand: n.demand,
    capacity: n.capacity,
    capacityFrom: n.capacityFrom,
    budget: n.budget,
    fill,
    headroom: n.demand && n.capacity ? n.capacity.hi - n.demand.hi : null,
    sourced: supplies.length > 0 || supplied,
    at: n.at ?? null,
  };
}

/** An issue as the page draws it: the server's severity is its level (an error is red, a note amber). */
function issueOf(i: PowerIssue): Issue {
  return { id: i.id, level: i.severity === 'error' ? 'bad' : 'warn', where: i.subject, text: i.text, at: i.at ?? null };
}

/** The worst issue level of each node. */
export function levels(issues: Issue[]): Map<string, Level> {
  const out = new Map<string, Level>();
  for (const i of issues) if (out.get(i.id) !== 'bad') out.set(i.id, i.level);
  return out;
}

export { loadsText } from './format';
