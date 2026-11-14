// The pours and zones under the tracks and pads of the 2D view. The server fills the board's pours,
// planes and the routes file's zones as the Gerbers and the 3D view have them (`GET /api/pcb/pours`,
// crates/netsmith-workspace/src/pours.rs) and sends each net's fill on each layer as an SVG path;
// the page puts it into the board's drawing, between the board and the planes' outlines, so it is
// under every pad, track, via and label. It is the layer's colour (`pcbdraw.css`, never the net's)
// with transparency, and goes with the layer switch of its copper.

import type { PcbPours, PcbScene } from '../api';
import { planeTone } from './planes';

/** Which switch of the Layers menu a fill follows: the outer layers go with their copper, a plane with Planes. */
export type PourSide = 'front' | 'back' | 'inner';

export interface PourShape {
  /** KiCad's name of the layer: `F.Cu`, `In1.Cu`, `B.Cu`. */
  layer: string;
  side: PourSide;
  /** The tone of an inner layer (1 to 4, as the planes have it); 0 on an outer layer. */
  tone: number;
  net: string | null;
  /** An SVG path on the sheet in mm. */
  d: string;
}

/** Whether `pours` is the fill of the placement `scene` shows: the same board, layout and routes
 * file, and routed or not. The fill of the board is drawn while it is not (a part was moved, a
 * track drawn: the old fill is nearer the truth than none, and the new one is on its way), marked
 * `data-current="false"` until it is. */
export function fills(pours: PcbPours | null, scene: PcbScene | null): pours is PcbPours {
  return (
    !!pours &&
    !!scene &&
    pours.board === scene.board &&
    pours.layoutStamp === scene.layoutStamp &&
    pours.routesStamp === scene.routesStamp &&
    pours.routed === scene.routed
  );
}

/** The side of a layer. A name that is neither outer layer is an inner one. */
export function sideOf(layer: string): PourSide {
  return layer === 'F.Cu' ? 'front' : layer === 'B.Cu' ? 'back' : 'inner';
}

/** Where a layer sits in the drawing: the inner layers by their number, then the back, then the front, so what is nearer the viewer is over. */
function rank(layer: string): number {
  const inner = /^In(\d+)\.Cu$/.exec(layer);
  if (inner) return Number(inner[1]);
  return layer === 'B.Cu' ? 1000 : layer === 'F.Cu' ? 2000 : 500;
}

/** The fill as shapes, in the order they are drawn: the deepest layer first. */
export function shapesOf(pours: PcbPours): PourShape[] {
  return [...pours.layers]
    .sort((a, b) => rank(a.layer) - rank(b.layer))
    .flatMap((l, i) => {
      const side = sideOf(l.layer);
      const tone = side === 'inner' ? planeTone(l.layer, i) : 0;
      return l.nets.filter((n) => n.d !== '').map((n) => ({ layer: l.layer, side, tone, net: n.net, d: n.d }));
    });
}

const escaped = (text: string): string =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** The fill as the group of the drawing, or an empty string when there is none. Each path is a
 * `.pour` with its layer, side and net (`data-layer`, `pour-front`, `pour-back`, `pour-inner` with
 * `data-plane`, `data-net`). `current` says whether it is the fill of the placement drawn. */
export function poursMarkup(pours: PcbPours | null, current: boolean): string {
  if (!pours) return '';
  const shapes = shapesOf(pours);
  if (shapes.length === 0) return '';
  const paths = shapes.map((s) => {
    const net = s.net === null ? '' : ` data-net="${escaped(s.net)}"`;
    const tone = s.side === 'inner' ? ` data-plane="${s.tone}"` : '';
    return `<path class="pour pour-${s.side}" data-layer="${escaped(s.layer)}"${tone}${net} d="${escaped(s.d)}"/>`;
  });
  return `<g class="l-pours" data-testid="pours" data-current="${current}">${paths.join('')}</g>`;
}

/** What each switch of the Layers menu hides of the fill (the rules go with the others in `Pcb.tsx`). */
export const POUR_HIDDEN: Record<string, string> = {
  'front-copper': '.pour-front',
  'back-copper': '.pour-back',
  planes: '.pour-inner',
};
