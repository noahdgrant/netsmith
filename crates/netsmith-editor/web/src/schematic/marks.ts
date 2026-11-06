// Which parts of the drawing the shared selection marks (the `mark` class: a halo, `halo.ts`, and the dimming of the rest).
//
// A picked part marks itself; a picked module marks every part under it, so that the parts of a module show where it is: on a sheet
// that draws other modules too, and in the overview, whose cards are the sheets. But the sheet of the module itself, when it is the one
// on screen, is the module: every part on it would wear the halo, which says nothing and boxes the whole drawing in grey (the module
// chip and the inspector already say it is selected). So there the module marks nothing. A part picked inside it still does, and what
// the source's cursor line declares is marked (less strongly, `linked`) whatever is selected.

export interface MarkInput {
  /** The selected part's designator, if a part is selected. */
  part: string | null;
  /** The selected module's instance path, if a module is selected. */
  module: string | null;
  /** The designators of the parts under the selected module. */
  moduleParts: readonly string[];
  /** The module whose sheet is on screen (`brain`); null for the overview of all modules, a board drawn as one page, or a sheet that is no module's. */
  shownModule: string | null;
  /** The designators of the parts the cursor's line in the source declares. */
  cursor: readonly string[];
}

export interface Marks {
  /** What the selection stands for, drawn as picked. */
  picked: string[];
  /** Picked and cursor parts, without repeats: the parts that wear the mark. */
  marked: string[];
}

/** Whether the selected module is the one whose sheet is on screen, so that it is not drawn as marked parts. */
export const moduleIsShown = (module: string | null, shownModule: string | null): boolean => !!module && module === shownModule;

export function marksOf(i: MarkInput): Marks {
  const byModule = i.module && !moduleIsShown(i.module, i.shownModule) ? [...i.moduleParts] : [];
  const picked = i.part ? [i.part] : byModule;
  return { picked, marked: [...new Set([...picked, ...i.cursor])] };
}
