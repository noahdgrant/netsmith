// The muted hints at the end of a line of the source: the resolved value and the chosen catalog part
// of a declared part (`10u · 0402 · C315248`), and the solved voltage of a rail where the source names
// its net (`.pos.net &= "3V3"`). Chosen from what the board's detail and the power tree already say;
// nothing is computed. Pure.

import type { BoardDetail, PartInfo, PowerTree } from '../api';
import { spaceUnits } from '../util';

export interface Hint {
  /** 1-based line the hint ends. */
  line: number;
  text: string;
  /** The longer wording, for the tooltip of the hint. */
  title: string;
}

/** What one part says: value, package and the supplier number it was resolved to. */
export function partText(p: PartInfo): string {
  return [p.value, p.package, p.lcsc].filter((x): x is string => !!x && x.trim() !== '').join(' · ');
}

/** The hints of the parts declared in `path`: one for each declaration line. A declaration that
 * stands for several parts (a module placed twice) shows the shared text with `x2`, or says they differ. */
export function partHints(detail: BoardDetail | null, path: string): Hint[] {
  const byLine = new Map<number, PartInfo[]>();
  for (const p of detail?.parts ?? []) {
    if (!p.decl || p.decl.file !== path) continue;
    const list = byLine.get(p.decl.line) ?? [];
    list.push(p);
    byLine.set(p.decl.line, list);
  }
  const out: Hint[] = [];
  for (const [line, parts] of byLine) {
    const texts = [...new Set(parts.map(partText))].filter(Boolean);
    if (texts.length === 0) continue;
    if (parts.length === 1) {
      out.push({ line, text: texts[0], title: `${parts[0].refdes}: ${texts[0]}` });
    } else if (texts.length === 1) {
      out.push({ line, text: `${texts[0]} ×${parts.length}`, title: `${parts.map((p) => p.refdes).join(', ')}: ${texts[0]}` });
    } else {
      out.push({ line, text: `${parts.length} instances, ${texts.length} values`, title: parts.map((p) => `${p.refdes}: ${partText(p)}`).join('\n') });
    }
  }
  return out;
}

const NET_LINE = /\.net\s*&?=\s*"([^"]+)"/;

/** The hints of the lines that name a net whose rail the power tree solved. */
export function netHints(lines: readonly string[], power: PowerTree | null): Hint[] {
  if (!power) return [];
  const out: Hint[] = [];
  lines.forEach((text, i) => {
    const m = NET_LINE.exec(text);
    if (!m) return;
    const node = power.nodes.find((n) => n.net === m[1]);
    const v = node?.voltage?.text ? spaceUnits(node.voltage.text) : undefined;
    if (!node || !v) return;
    const demand = node.demand?.text ? spaceUnits(node.demand.text) : undefined;
    out.push({ line: i + 1, text: v, title: `${m[1]}: ${v}${demand ? `, draws ${demand}` : ''}` });
  });
  return out;
}

/** Every hint of a file, one for each line at most (a part's wins over a rail's), in line order. */
export function inlineHints(path: string, lines: readonly string[], detail: BoardDetail | null, power: PowerTree | null): Hint[] {
  const all = new Map<number, Hint>();
  for (const h of netHints(lines, power)) all.set(h.line, h);
  for (const h of partHints(detail, path)) if (h.line >= 1 && h.line <= lines.length) all.set(h.line, h);
  return [...all.values()].sort((a, b) => a.line - b.line);
}
