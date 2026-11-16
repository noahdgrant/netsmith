// The placed board summed up (issue 1003): what the Summary tab asks the server for and how it
// reads the answer. The numbers are the server's (`netsmith layout summary` has the same); this
// file only turns them into the words and boxes the tab shows.

import type { PcbBox, PcbScene, SummaryBox, SummaryGuideline, SummaryPads } from '../api';

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
