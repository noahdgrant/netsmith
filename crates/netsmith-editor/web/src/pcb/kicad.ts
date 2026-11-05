// What the findings list says of KiCad's check. KiCad is optional (netsmith's own checks need
// none), so the line says which of four things is so; the page and the recipe that drives it
// read the same table.

import type { KicadState } from '../api';

/** KiCad's command-line tool is not on the machine (nor named by `$KICAD_CLI`). */
export const KICAD_NOT_FOUND = 'KiCad not found: set KICAD_CLI or install KiCad';

/** The line next to "Also ask KiCad". `found` is whether the server found `kicad-cli` (the
 * project's `kicad` flag), or null while that is not known. A check that ran says so; where the
 * tool is not found that is the line, whether or not it was asked for; a tool that is there but
 * not asked yet is "not run", and one that is there but failed is "unavailable" (the note says why). */
export function kicadLine(state: KicadState, found: boolean | null): string {
  if (state === 'ran') return 'with KiCad';
  if (found === false) return KICAD_NOT_FOUND;
  return state === 'unavailable' ? 'KiCad unavailable' : 'KiCad not run';
}

/** Every line `kicadLine` can give for a state: what a test of the page may accept for the state
 * the server reports (the `found` flag is not known to every caller). */
export function kicadLinesFor(state: KicadState): string[] {
  const lines = new Set([true, false, null].map((found) => kicadLine(state, found)));
  return [...lines];
}
