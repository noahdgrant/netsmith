// What the fill of the pours took out and the pads it starves, over the 2D view's copper (issue 1188): the server's report of the
// fill (`GET /api/pcb/fill`, crates/netsmith-pcb/src/fill_report.rs, what `netsmith board fill` prints), drawn where it is. Each
// copper island the fill removed (copper of a pour that touched no copper of its net, so the Gerbers have none there) is hatched
// and outlined, dashed, in `--pcb-island`; each pad the fill starves (fewer than two spokes and tracks join it to the pour, the
// design rules check's `starved_thermal`) is ringed in `--pcb-starved`, as `netsmith view pcb --islands --drc` marks them. The
// places are in the board file's frame, which is the sheet's, so they go into the drawing as they are.

import type { PcbFill, PcbScene } from '../api';
import { type PourSide, sideOf } from './pours';

/** Whether `fill` is of the placement `scene` shows, as `fills` says of the pours. */
export function fillOf(fill: PcbFill | null, scene: PcbScene | null): fill is PcbFill {
  return (
    !!fill &&
    !!scene &&
    fill.board === scene.board &&
    fill.layoutStamp === scene.layoutStamp &&
    fill.routesStamp === scene.routesStamp &&
    fill.routed === scene.routed
  );
}

/** A removed island as it is drawn. */
export interface IslandShape {
  /** KiCad's name of its layer, and the side of it. */
  layer: string;
  side: PourSide;
  net: string;
  area: number;
  /** An SVG path on the sheet in mm: the outline, then its holes. */
  d: string;
}

/** A starved pad as it is drawn. */
export interface StarvedShape {
  layer: string;
  side: PourSide;
  net: string;
  pad: string;
  x: number;
  y: number;
  /** `1 of 4 spokes, 0 tracks`. */
  how: string;
}

const num = (v: number): string => String(Math.round(v * 1000) / 1000);

/** Every removed island with an outline, in the report's order. */
export function islandsOf(fill: PcbFill): IslandShape[] {
  return fill.fills.flatMap((f) =>
    f.removedIslands
      .map((i) => ({
        layer: f.layerName,
        side: sideOf(f.layerName),
        net: f.net,
        area: i.area,
        d: i.outline
          .filter((r) => r.length >= 3)
          .map((r) => r.map((p, k) => `${k === 0 ? 'M' : 'L'}${num(p.x)} ${num(p.y)}`).join('') + 'Z')
          .join(''),
      }))
      .filter((s) => s.d !== ''),
  );
}

/** Every starved pad, with KiCad's name of its layer. */
export function starvedOf(fill: PcbFill): StarvedShape[] {
  const names = new Map(fill.fills.map((f) => [f.layer, f.layerName]));
  return fill.starved.map((s) => {
    const layer = names.get(s.layer) ?? s.layer;
    return {
      layer,
      side: sideOf(layer),
      net: s.net,
      pad: `${s.part}.${s.pad}`,
      x: s.at.x,
      y: s.at.y,
      how: `${s.joinedSpokes} of ${s.spokes} spokes, ${s.tracks} track${s.tracks === 1 ? '' : 's'}`,
    };
  });
}

const escaped = (text: string): string =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** The marks as groups of the drawing (`l-islands`, `l-starved`), or an empty string when there are none. `current` says whether
 * the fill is of the placement drawn: the marks of an older one stay, marked `data-current="false"`, until the new one comes. */
export function fillMarkup(fill: PcbFill | null, current: boolean): string {
  if (!fill) return '';
  const islands = islandsOf(fill).map(
    (s) =>
      `<path class="removed-island island-${s.side}" data-layer="${escaped(s.layer)}" data-net="${escaped(s.net)}" data-area="${num(s.area)}" d="${s.d}"><title>A copper island of the ${escaped(s.net)} pour that the fill took out: ${num(s.area)} mm²</title></path>`,
  );
  const starved = starvedOf(fill).map(
    (s) =>
      `<g class="starved-pad starved-${s.side}" data-layer="${escaped(s.layer)}" data-net="${escaped(s.net)}" data-pad="${escaped(s.pad)}"><title>${escaped(s.pad)}: the ${escaped(s.net)} pour starves it (${s.how}; it needs 2)</title><circle class="starved-ring" cx="${num(s.x)}" cy="${num(s.y)}" r="1.2"/><circle class="starved-dot" cx="${num(s.x)}" cy="${num(s.y)}" r=".25"/></g>`,
  );
  const group = (cls: string, testid: string, items: string[]) =>
    items.length ? `<g class="${cls}" data-testid="${testid}" data-current="${current}">${items.join('')}</g>` : '';
  return group('l-islands', 'removed-islands', islands) + group('l-starved', 'starved-pads', starved);
}
