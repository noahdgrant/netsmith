// What the placer and the router of a board have made so far, as the server shows it while they run
// (`pcb_live` on the socket, issue 764): the parts where the placer has them, drawn on their own while
// the board is placed, and the copper the router has laid, drawn over the board while it is routed.

import type { PcbLive } from '../api';

const escaped = (text: string): string =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** A number for an attribute: a hundredth of a millimetre is finer than the screen shows. */
const n = (v: number): string => String(Math.round(v * 100) / 100);

/** The parts of a `place` glimpse as an SVG of their own: the region the placer places them in, each part the box round its
 * courtyard (the back face apart), with its designator when it has room for it. Empty for any other glimpse. */
export function placingSvg(live: PcbLive | null): string {
  if (!live || live.task !== 'place' || !live.region) return '';
  const r = live.region;
  // The box round the region and every part: a part the placer has outside the region is still drawn.
  let [x0, y0, x1, y1] = [r.x, r.y, r.x + r.width, r.y + r.height];
  for (const p of live.parts) {
    x0 = Math.min(x0, p.box.x);
    y0 = Math.min(y0, p.box.y);
    x1 = Math.max(x1, p.box.x + p.box.width);
    y1 = Math.max(y1, p.box.y + p.box.height);
  }
  const pad = Math.max(1, (x1 - x0 + y1 - y0) * 0.02);
  const view = [x0 - pad, y0 - pad, x1 - x0 + 2 * pad, y1 - y0 + 2 * pad].map(n).join(' ');
  const parts = live.parts.map((p) => {
    const b = p.box;
    const size = Math.min(b.width, b.height) * 0.45;
    const label =
      size >= 0.4 && b.width >= p.refdes.length * size * 0.6
        ? `<text x="${n(b.x + b.width / 2)}" y="${n(b.y + b.height / 2)}" font-size="${n(size)}">${escaped(p.refdes)}</text>`
        : '';
    return `<g class="live-part ${p.back ? 'back' : 'front'}" data-ref="${escaped(p.refdes)}"><rect x="${n(b.x)}" y="${n(b.y)}" width="${n(b.width)}" height="${n(b.height)}"/>${label}</g>`;
  });
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${view}" preserveAspectRatio="xMidYMid meet"><rect class="live-region" x="${n(r.x)}" y="${n(r.y)}" width="${n(r.width)}" height="${n(r.height)}"/>${parts.join('')}</svg>`;
}

/** The copper of a `route` glimpse as a group for the board's drawing (its coordinates are the drawing's), drawn over
 * everything. Empty for any other glimpse. */
export function routingMarkup(live: PcbLive | null): string {
  if (!live || live.task !== 'route') return '';
  const tracks = live.tracks.map(
    (t) =>
      `<line class="live-track" data-layer="${escaped(t.layer)}" x1="${n(t.x1)}" y1="${n(t.y1)}" x2="${n(t.x2)}" y2="${n(t.y2)}" stroke-width="${n(t.width)}"/>`,
  );
  const vias = live.vias.map((v) => `<circle class="live-via" cx="${n(v.x)}" cy="${n(v.y)}" r="${n(v.diameter / 2)}"/>`);
  return `<g class="l-live" data-testid="pcb-live-tracks" data-tracks="${live.tracks.length}">${tracks.join('')}${vias.join('')}</g>`;
}

/** The words for a placing in hand: the last line the placer told, or what is being done. */
export function placingLine(line: string | null, live: PcbLive | null): string {
  if (line) return line;
  if (live?.task === 'place') return `placing ${live.parts.length} parts`;
  return 'placing the board';
}
