// A plane's layer. The server's drawing gives each plane rectangle a `data-layer` (`In1.Cu`); the page colours the plane by that layer
// (`--pcb-plane-1` to `--pcb-plane-4`) and names it. A server that does not send the attribute (an older netsmith) leaves the page
// to guess by position, the n-th rectangle on the n-th layer, which is what round 5 did.

/** The number of tones the theme has for inner layers. */
export const PLANE_TONES = 4;

/** The tone (1 to 4) of a plane: by its layer when the layer is an inner one (`In3.Cu` is the third tone, `In4.Cu` and deeper the fourth),
 * by its place among the planes (`index`, from 0) when the layer is not known or is an outer one. */
export function planeTone(layer: string | null | undefined, index: number): number {
  const inner = layer ? /^In(\d+)\.Cu$/.exec(layer) : null;
  const n = inner ? Number(inner[1]) : index + 1;
  return Math.min(Math.max(n, 1), PLANE_TONES);
}

/** What the plane's label says: its net, and its layer when the drawing names it (`GND · In1.Cu`). */
export function planeLabel(net: string, layer: string | null | undefined): string {
  return layer ? `${net} · ${layer}` : net;
}
