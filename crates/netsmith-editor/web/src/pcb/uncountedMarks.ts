// The copper a net's length does not count (issue 1495), over the 2D view's copper: the runs of the table of the nets
// (`GET /api/pcb/nets`, `netsmith_fab::NetTable`, what `netsmith board nets` lists as `not counted in its length`). A dead
// stub (copper that leads to no pad) and a second way (copper that joins pads the connection already joins by a shorter way)
// are drawn over their track in `--pcb-uncounted`, dashed, so they read apart from the copper the length counts. Their switch
// is the Layers menu's `Not counted`. A piece that lies on a track of the routes file names it (`data-route-net`,
// `data-route-track`), so a click picks that track as a click on the track does; a piece of the router's copper names its net
// (`data-net`), so a click picks the net. The places are in the board file's frame, which is the sheet's, so they go into the
// drawing as they are.

import type { NetsUncounted, PcbNets, PcbRouteNet, PcbScene } from '../api';

/** Whether `nets` is of the placement `scene` shows, as `fillOf` says of the fill. */
export function netsOf(nets: PcbNets | null, scene: PcbScene | null): nets is PcbNets {
  return (
    !!nets &&
    !!scene &&
    nets.board === scene.board &&
    nets.layoutStamp === scene.layoutStamp &&
    nets.routesStamp === scene.routesStamp &&
    nets.routed === scene.routed
  );
}

/** The words of a kind of copper not counted: `a dead stub`, `a second way`, as `board nets` says them. */
export function uncountedWords(kind: NetsUncounted['kind']): string {
  return kind === 'stub' ? 'a dead stub' : 'a second way';
}

/** The routes file's word for KiCad's name of a copper layer: `F.Cu` is `top`, `B.Cu` `bottom`, `In2.Cu` `inner2`. */
export function routesLayer(layer: string): string {
  if (layer === 'F.Cu') return 'top';
  if (layer === 'B.Cu') return 'bottom';
  const inner = /^In(\d+)\.Cu$/.exec(layer);
  return inner ? `inner${inner[1]}` : layer;
}

type P = [number, number];

function toSegment(p: P, a: { x: number; y: number }, b: { x: number; y: number }): number {
  const [dx, dy] = [b.x - a.x, b.y - a.y];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a.x) * dx + (p[1] - a.y) * dy) / len2));
  return Math.hypot(p[0] - (a.x + dx * t), p[1] - (a.y + dy * t));
}

/** The number (from 1, as `netsmith routes show` numbers them) of the straight track of the routes file's record `record` on
 * `layer` (KiCad's name) that the piece `from`-`to` lies on, or null when it is on none: the router's copper. */
export function routesTrackOf(record: PcbRouteNet | undefined, layer: string, from: P, to: P): number | null {
  if (!record) return null;
  const word = routesLayer(layer);
  const i = record.tracks.findIndex(
    (t) => !t.mid && t.layer === word && Math.max(toSegment(from, t.from, t.to), toSegment(to, t.from, t.to)) <= t.width / 2 + 1e-3,
  );
  return i < 0 ? null : i + 1;
}

const num = (v: number): string => String(Math.round(v * 1000) / 1000);

const escaped = (text: string): string =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

const sideOf = (layer: string): string => (layer === 'F.Cu' ? 'front' : layer === 'B.Cu' ? 'back' : 'inner');

/** The runs not counted of every net as one group of the drawing (`l-uncounted`), or an empty string when there are none.
 * `current` says whether they are of the placement drawn: those of an older one stay, marked `data-current="false"`, until the
 * new ones come. `routes` are the routes file's records, so a piece on one of their tracks names it. */
export function uncountedMarkup(nets: PcbNets | null, current: boolean, routes: PcbRouteNet[]): string {
  if (!nets) return '';
  const runs: string[] = [];
  for (const n of nets.nets) {
    const record = routes.find((r) => r.net === n.net);
    for (const run of n.notCounted ?? []) {
      const title = `<title>${escaped(`${uncountedWords(run.kind)} of ${n.net}, ${num(run.length)} mm on ${run.layer}: not counted in its length`)}</title>`;
      const pieces = run.pieces.map((p) => {
        const track = routesTrackOf(record, run.layer, p.from, p.to);
        const names = track ? `data-route-net="${escaped(n.net)}" data-route-track="${track}"` : `data-net="${escaped(n.net)}"`;
        return `<line class="uncounted-piece" ${names} x1="${num(p.from[0])}" y1="${num(p.from[1])}" x2="${num(p.to[0])}" y2="${num(p.to[1])}" stroke-width="${num(p.width)}">${title}</line>`;
      });
      runs.push(
        `<g class="uncounted ${run.kind === 'stub' ? 'stub' : 'second-way'} uncounted-${sideOf(run.layer)}" data-net="${escaped(n.net)}" data-kind="${run.kind}" data-layer="${escaped(run.layer)}" data-length="${num(run.length)}">${pieces.join('')}</g>`,
      );
    }
  }
  if (runs.length === 0) return '';
  return `<g class="l-uncounted" data-testid="uncounted-copper" data-count="${runs.length}" data-current="${current}">${runs.join('')}</g>`;
}
