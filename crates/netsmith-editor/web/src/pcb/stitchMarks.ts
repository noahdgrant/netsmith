// The stitching vias the export will have that the routes file does not (issue 1489), over the 2D view's copper: the server's list
// (`GET /api/pcb/stitching`, `netsmith_router::stitching_vias`, the stitching `fab` runs, what `netsmith view pcb --stitching`
// draws). Each is a disc in `--pcb-stitching` with a dashed ring, a tone apart from the vias of the copper drawn, so it reads as copper
// the export will lay rather than copper the routes file holds. Their switch is the Layers menu's Stitching. The places are in the
// board file's frame, which is the sheet's, so they go into the drawing as they are.

import type { PcbScene, PcbStitching } from '../api';

/** Whether `stitching` is of the placement `scene` shows, as `fillOf` says of the fill. */
export function stitchingOf(stitching: PcbStitching | null, scene: PcbScene | null): stitching is PcbStitching {
  return (
    !!stitching &&
    !!scene &&
    stitching.board === scene.board &&
    stitching.layoutStamp === scene.layoutStamp &&
    stitching.routesStamp === scene.routesStamp &&
    stitching.routed === scene.routed
  );
}

const num = (v: number): string => String(Math.round(v * 1000) / 1000);

const escaped = (text: string): string =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** The vias as one group of the drawing (`l-stitching`), or an empty string when there are none. `current` says whether they are of
 * the placement drawn: those of an older one stay, marked `data-current="false"`, until the new ones come. */
export function stitchingMarkup(stitching: PcbStitching | null, current: boolean): string {
  if (!stitching || stitching.vias.length === 0) return '';
  const vias = stitching.vias.map(
    (v) =>
      `<circle class="stitching-via" data-net="${escaped(v.net)}" cx="${num(v.x)}" cy="${num(v.y)}" r="${num(v.diameter / 2)}"><title>A stitching via of ${escaped(v.net)} the export will lay: not in the routes file</title></circle>`,
  );
  return `<g class="l-stitching" data-testid="stitching-vias" data-count="${vias.length}" data-current="${current}">${vias.join('')}</g>`;
}
