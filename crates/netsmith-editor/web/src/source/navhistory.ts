// The navigation history of the source: the places the cursor was taken to (a go to definition, a
// click on a part in another view, the outline, a problem) and Back and Forward through them, like a
// browser's. A stack and an index: going somewhere forgets what was ahead, going back and forward
// moves the index. Pure.

export interface Place {
  file: string;
  line: number;
  column: number;
}

export interface History {
  entries: Place[];
  /** The entry the cursor is at. -1 when there is none. */
  index: number;
}

export const EMPTY: History = { entries: [], index: -1 };

export const LIMIT = 60;

/** Two places that a person would not call two: the same file and the same line. */
export const samePlace = (a: Place, b: Place): boolean => a.file === b.file && a.line === b.line;

/** A new place, after the one the history is at. The same line as the current one changes nothing
 * (it only moves the column); anything ahead is dropped. */
export function visit(h: History, to: Place): History {
  const here = h.entries[h.index];
  if (here && samePlace(here, to)) {
    const entries = h.entries.slice();
    entries[h.index] = to;
    return { entries, index: h.index };
  }
  const kept = h.entries.slice(0, h.index + 1);
  kept.push(to);
  const entries = kept.length > LIMIT ? kept.slice(kept.length - LIMIT) : kept;
  return { entries, index: entries.length - 1 };
}

/** Where the cursor is now, before it is taken somewhere: recorded so Back returns to it. */
export function leaving(h: History, from: Place | null): History {
  return from ? visit(h, from) : h;
}

export const canBack = (h: History): boolean => h.index > 0;
export const canForward = (h: History): boolean => h.index >= 0 && h.index < h.entries.length - 1;

/** One step back, and the place it reaches; null at the start. */
export function stepBack(h: History): { history: History; place: Place } | null {
  if (!canBack(h)) return null;
  const index = h.index - 1;
  return { history: { ...h, index }, place: h.entries[index] };
}

export function stepForward(h: History): { history: History; place: Place } | null {
  if (!canForward(h)) return null;
  const index = h.index + 1;
  return { history: { ...h, index }, place: h.entries[index] };
}

/** Files that were renamed or removed: their places go. */
export function forget(h: History, gone: (file: string) => boolean): History {
  const entries: Place[] = [];
  let index = -1;
  h.entries.forEach((e, i) => {
    if (gone(e.file)) return;
    entries.push(e);
    if (i <= h.index) index = entries.length - 1;
  });
  return { entries, index };
}
