// The placed board summed up (issue 1003): what the Summary tab asks the server for and how it
// reads the answer. The numbers are the server's (`netsmith layout summary` has the same); this
// file only turns them into the words and boxes the tab shows.

import type { CopperLayer, CopperPour, CopperZone, FillArea, FillCounts, FillPad, FillRegulatorGround, FillStarved, NetsGroup, NetsPair, NetsRow, NetsUncounted, PcbBox, PcbCopper, PcbScene, SummaryBox, SummaryGuideline, SummaryPads } from '../api';

/** A number to the hundredth, without trailing zeros: `3.5`, `12`. */
export function mm(v: number): string {
  return String(Math.round(v * 100) / 100);
}

/** A box of the summary (mm from the outline's top left corner) as the box of the sheet that the canvas zooms to. */
export function sheetBox(scene: PcbScene, b: SummaryBox): PcbBox {
  return { x: scene.originX + b.min[0], y: scene.originY + b.min[1], width: b.max[0] - b.min[0], height: b.max[1] - b.min[1] };
}

/** What the person typed for a pad, `R1.2`: a part (its designator or path), a dot and the pad's number; null when it is not that. */
export function padName(text: string): string | null {
  const t = text.trim();
  const dot = t.lastIndexOf('.');
  return dot > 0 && dot < t.length - 1 && !/[\s,]/.test(t) ? t : null;
}

/** Why a pad named by hand is not one of the board's, in the server's words, or null when it is: the part is one of the scene's (by designator or path) and has a pad of that number. The page asks before the server does, so a wrong name is no failed request. */
export function padProblem(scene: PcbScene, name: string): string | null {
  const dot = name.lastIndexOf('.');
  const [part, number] = [name.slice(0, dot), name.slice(dot + 1)];
  const found = scene.parts.find((p) => p.path === part || p.refdes === part);
  if (!found) return `the board \`${scene.board}\` has no part \`${part}\``;
  if (found.pads.some((p) => p.number === number)) return null;
  const numbers = [...new Set(found.pads.map((p) => p.number))];
  return `the part \`${found.refdes}\` has no pad \`${number}\`; its pads are: ${numbers.join(', ')}`;
}

/** The pairs of pads typed in, parted into those to ask the server about and, for those the board has no pad for any more (a source
 * edit took the part or the pad away), why each is left out. A pair left out stays typed, and is asked about again once the board has it. */
export function pairsOnBoard(scene: PcbScene, pairs: [string, string][]): { asked: [string, string][]; leftOut: string[] } {
  const asked: [string, string][] = [];
  const leftOut: string[] = [];
  for (const [a, b] of pairs) {
    const wrong = padProblem(scene, a) ?? padProblem(scene, b);
    if (wrong) leftOut.push(`${a} to ${b} is not measured: ${wrong}`);
    else asked.push([a, b]);
  }
  return { asked, leftOut };
}

/** The query of `GET /api/pcb/summary`: the board, the parts whose neighbours to list, how far, and the pairs of pads. */
export function summaryQuery(board: string, near: string[], within: number, between: [string, string][]): string {
  const q = new URLSearchParams({ board });
  for (const n of near) q.append('near', n);
  if (near.length > 0) q.set('within', String(within));
  for (const [a, b] of between) q.append('between', `${a},${b}`);
  return q.toString();
}

/** The distance between two pads in words: `1.5 mm between centres, 0.4 mm between the copper`, and the net when they share one. */
export function padsLine(p: SummaryPads): string {
  const nets = p.fromNet && p.fromNet === p.toNet ? `, both on ${p.fromNet}` : [p.fromNet && ` ${p.from} on ${p.fromNet}`, p.toNet && ` ${p.to} on ${p.toNet}`].filter(Boolean).join(',');
  return `${mm(p.distance)} mm between centres, ${mm(p.gap)} mm between the copper${nets ? (nets.startsWith(',') ? nets : `,${nets}`) : ''}`;
}

/** The fill of a face as a percentage: `34%`. */
export function fillWord(fill: number): string {
  return `${Math.round(fill * 100)}%`;
}

/** What a datasheet's layout guideline measures, in words (issue 994): `(near, C1) 1.2 mm, at most 2 mm, met`. A count of vias has no unit. */
export function guidelineLine(g: SummaryGuideline): string {
  const unit = g.statement === 'vias' ? '' : ' mm';
  const parts = [g.measured === undefined ? 'not measured' : `${mm(g.measured)}${unit}`];
  if (g.atMost !== undefined) parts.push(`at most ${mm(g.atMost)}${unit}`);
  if (g.atLeast !== undefined) parts.push(`at least ${mm(g.atLeast)}${unit}`);
  if (g.met === true) parts.push('met');
  if (g.met === false) parts.push('not met');
  return `(${g.statement}${g.other ? `, ${g.other}` : ''}) ${parts.join(', ')}`;
}

/** A pour's fields in words, `stitch pitch 7 mm, thermal gap default, ...`: a field the design does not state is the process's default. */
export function pourFields(p: CopperPour): string {
  const length = (name: string, v: number | null) => `${name} ${v === null ? 'default' : `${mm(v)} mm`}`;
  return [
    length('stitch pitch', p.stitchPitch),
    length('thermal gap', p.thermalGap),
    length('spoke width', p.spokeWidth),
    length('clearance', p.clearance),
    length('min width', p.minWidth),
    `pads ${p.pads ?? 'default'}`,
  ].join(', ');
}

/** One copper layer in words: `plane on GND`, `pour on GND: stitch pitch 7 mm, ...` or `signal`, and what the routes file's zones do to it. */
export function copperLayerLine(l: CopperLayer): string {
  const head = `${l.kind}${l.net ? ` on ${l.net}` : ''}${l.pour ? `: ${pourFields(l.pour)}` : ''}`;
  if (l.replacedByZones) return `${head} (not drawn: ${l.zones} zone${l.zones === 1 ? '' : 's'} of the routes file replace${l.zones === 1 ? 's' : ''} it)`;
  if (l.zones > 0 && l.kind !== 'signal') return `${head} (cut by ${l.zones} hand zone${l.zones === 1 ? '' : 's'} of other nets, drawn round them)`;
  if (l.zones > 0) return `${head} (${l.zones} hand zone${l.zones === 1 ? '' : 's'})`;
  return head;
}

/** A zone of the routes file in words: `GND on top: 24 mm², priority 0, clearance 0.2 mm, pads thermal, locked`. */
export function copperZoneLine(z: CopperZone): string {
  return [
    `${z.net} on ${z.layers.join(', ')}: ${mm(z.area)} mm²`,
    `priority ${z.priority}`,
    `clearance ${mm(z.clearance)} mm`,
    `min width ${mm(z.minWidth)} mm`,
    `pads ${z.pads}`,
    ...(z.locked ? ['locked'] : []),
  ].join(', ');
}

/** The teardrop fills in words: `not counted until the board is routed`, or `412 (230 on top, 182 on bottom)`. Teardrops are zones of the KiCad board, and none is a pour. */
export function teardropsLine(c: PcbCopper): string {
  const t = c.teardrops;
  if (!t.counted) return 'not counted: the router makes them when it routes the board';
  const by = t.layers.map((l) => `${l.count} on ${l.layer}`).join(', ');
  return `${t.total}${by ? ` (${by})` : ''}`;
}

/** The pads a fill joins, counted by kind (issue 1187, `netsmith board fill`): `12 pads: 9 by 2 or more spokes, 1 by 1 spoke, 0 by no spoke, 2 solid, 0 none; 1 starved, 0 unjoinable`. */
export function fillCountsLine(c: FillCounts): string {
  const pads = c.noSpokes + c.oneSpoke + c.twoOrMoreSpokes + c.solid + c.none;
  return `${pads} pad${pads === 1 ? '' : 's'}: ${c.twoOrMoreSpokes} by 2 or more spokes, ${c.oneSpoke} by 1 spoke, ${c.noSpokes} by no spoke, ${c.solid} solid, ${c.none} none; ${c.starved} starved, ${c.unjoinable} unjoinable`;
}

/** A fill in words: `pour on GND, 120.5 mm²: <counts>; clears 4 pads of other nets; islands: 0 kept, 1 removed`. */
export function fillAreaLine(f: FillArea): string {
  return [
    `${f.source} on ${f.net}${f.held ? ' (the fill the board file holds)' : ''}, ${mm(f.area)} mm²: ${fillCountsLine(f.counts)}`,
    `clears ${f.cleared.length} pad${f.cleared.length === 1 ? '' : 's'} of other nets`,
    `islands: ${f.islands.length} kept, ${f.removedIslands.length} removed`,
  ].join('; ');
}

/** How the fill joins a pad: `U1.2: thermal, 1 of 4 spokes (starved)`, `J1.1: solid (as its pin radio.gnd states)`, `J1.2: thermal, 0 of 4 spokes
 * (unjoinable: <why>)` or `(joined by another fill of its net)`. The words are `board fill`'s. */
export function fillPadLine(p: FillPad): string {
  let how: string = p.join;
  if (p.join === 'thermal') how = p.spokes === 0 ? 'thermal, no spokes (the pad is narrower than the minimum width)' : `thermal, ${p.joinedSpokes} of ${p.spokes} spokes`;
  else if (p.join === 'solid' && !p.joined) how = 'solid, the fill does not reach it';
  const tracks = p.tracks === 0 ? '' : ` and ${p.tracks} track${p.tracks === 1 ? '' : 's'}`;
  // A pad its pin joins otherwise than the pour says (issue 1276).
  const own = p.statedBy === null ? '' : ` (as its pin ${p.statedBy} states${p.spokeWidth === null ? '' : `, ${mm(p.spokeWidth)} mm spokes`})`;
  let flag = '';
  if (p.unjoinable) flag = ` (unjoinable: ${p.unjoinable})`;
  else if (p.starved) flag = ' (starved)';
  else if (p.joinedElsewhere) flag = ' (joined by another fill of its net)';
  return `${p.part}.${p.pad}: ${how}${tracks}${own}${flag}`;
}

/** A starved thermal relief: `U1.2 on top (GND) at (8, 15): 1 of 4 spokes`. */
export function fillStarvedLine(s: FillStarved): string {
  const tracks = s.tracks === 0 ? '' : `, ${s.tracks} track${s.tracks === 1 ? '' : 's'}`;
  return `${s.part}.${s.pad} on ${s.layer} (${s.net}) at (${mm(s.at.x)}, ${mm(s.at.y)}): ${s.joinedSpokes} of ${s.spokes} spokes${tracks}`;
}

/** A switching regulator's ground pad a fill does not join solid (issue 1586), and the pin to state it solid on:
 * `U1.2 on top (GND) at (8, 15), the ground of the switching regulator reg.ic: a thermal relief, 4 of 4 spokes; ...`. The words are `board fill`'s. */
export function fillRegulatorGroundLine(g: FillRegulatorGround): string {
  let how = 'not joined (the fill\u2019s pad connection is none)';
  if (g.join === 'thermal') how = g.spokes === null || g.joinedSpokes === null ? 'a thermal relief' : `a thermal relief, ${g.joinedSpokes} of ${g.spokes} spokes${g.starved ? ', starved' : ''}`;
  else if (g.join === 'solid') how = 'solid, but the fill does not reach it';
  const pin = g.statedBy ?? `${g.path}.vin.neg`;
  return `${g.part}.${g.pad} on ${g.layer} (${g.net}) at (${mm(g.at.x)}, ${mm(g.at.y)}), the ground of the switching regulator ${g.path}: ${how}; a regulator\u2019s ground carries its switched current and its heat: join it solid with .pour_connection &= PadConnection::Solid on its pin ${pin}`;
}

/** A net of the table of the routed board (issue 1223, `netsmith board nets`): `CANH: 63.09 mm routed, straight 60.2 mm (1.05x), 2 vias, 2 layer changes, on F.Cu B.Cu`. */
export function netRowLine(n: NetsRow): string {
  const ratio = n.ratio > 0 ? ` (${n.ratio.toFixed(2)}x)` : '';
  const vias = `${n.vias} via${n.vias === 1 ? '' : 's'}, ${n.layerChanges} layer change${n.layerChanges === 1 ? '' : 's'}`;
  return `${n.net}: ${mm(n.length)} mm routed, straight ${mm(n.straight)} mm${ratio}, ${vias}${n.layers.length ? `, on ${n.layers.join(' ')}` : ''}`;
}

/** A diff pair of the table: `CANH/CANL: 63.09 and 63.11 mm, skew 0.02 mm (within 0.1 mm); coupled 60.5 mm (96%), gap 0.1 to 0.12 mm; vias 2 and 2, 2 side by side`. */
export function netPairLine(p: NetsPair): string {
  const gaps = p.gaps ? `, gap ${mm(p.gaps[0])} to ${mm(p.gaps[1])} mm` : '';
  return [
    `${p.p}/${p.n}: ${mm(p.lengthP)} and ${mm(p.lengthN)} mm, skew ${mm(p.skew)} mm (${p.over ? 'over' : 'within'} ${mm(p.limit)} mm)`,
    `coupled ${mm(p.coupled)} mm (${Math.round(p.share * 100)}%)${gaps}`,
    `vias ${p.viasP} and ${p.viasN}, ${p.viaPairs} side by side`,
  ].join('; ');
}

/** A group of nets matched in length (issue 1234): `bus.lanes: D0 40.1, D1 40.05, D2 39.8 mm, spread 0.3 mm (over 0.2 mm)`. */
export function netGroupLine(g: NetsGroup): string {
  const nets = g.nets.map((n) => `${n.net} ${mm(n.length)}`).join(', ');
  return `${g.path}: ${nets} mm, spread ${mm(g.spread)} mm (${g.over ? 'over' : 'within'} ${mm(g.tolerance)} mm)`;
}

/** What a net's length leaves out (issue 1495), as `board nets` says it: `P: 10 mm of its 30 mm of copper not counted in its length`; empty for a net with nothing left out. */
export function netUncountedLine(n: NetsRow): string {
  if (!n.notCounted?.length) return '';
  return `${n.net}: ${mm(n.copper - n.length)} mm of its ${mm(n.copper)} mm of copper not counted in its length`;
}

/** One run a net's length leaves out: `a second way of 10 mm on F.Cu at (12, 10)`. */
export function uncountedRunLine(r: NetsUncounted): string {
  return `${r.kind === 'stub' ? 'a dead stub' : 'a second way'} of ${mm(r.length)} mm on ${r.layer} at (${mm(r.at[0])}, ${mm(r.at[1])})`;
}

/** The box round a run's pieces, with a millimetre round it, to zoom to: the board file's frame is the sheet's. */
export function uncountedBox(r: NetsUncounted): PcbBox {
  const xs = r.pieces.flatMap((p) => [p.from[0], p.to[0]]).concat(r.at[0]);
  const ys = r.pieces.flatMap((p) => [p.from[1], p.to[1]]).concat(r.at[1]);
  const [x0, y0] = [Math.min(...xs) - 1, Math.min(...ys) - 1];
  return { x: x0, y: y0, width: Math.max(...xs) + 1 - x0, height: Math.max(...ys) + 1 - y0 };
}
