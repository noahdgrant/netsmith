// What the findings list says of KiCad's check. KiCad is optional (netsmith's own checks need
// none), so the line says which of four things is so; the page and the recipe that drives it
// read the same table.

import type { KicadState } from '../api';

/** KiCad's command-line tool is not on the machine (nor named by `$KICAD_CLI`): the findings
 * list says it once, in its own empty state (title `KiCad not found`, the fix as its help), and
 * has no line beside "Also ask KiCad" then. This is the message of that state, for the tests. */
export const KICAD_NOT_FOUND = 'KiCad not found';

/** The line next to "Also ask KiCad", for a tool that was found (where it is not found the
 * empty state says so and there is no line, so the list never says it twice). A check that ran
 * says so; a tool that is there but not asked yet is "not run", and one that is there but failed
 * is "unavailable" (the note says why). */
export function kicadLine(state: KicadState): string {
  if (state === 'ran') return 'with KiCad';
  return state === 'unavailable' ? 'KiCad unavailable' : 'KiCad not run';
}
