// Which sheet of a schematic is shown, and the view each one had: pure functions.

/** The pick that shows every sheet at once. */
export const ALL = '*';

/** The sheets in the order of the strip: the overview first. */
export const entries = (names: string[]) => [ALL, ...names];

/** What to show for a saved pick: the pick if the drawing still has it, else the first sheet of a
 * board that has more than one, else the whole drawing. */
export function resolvePick(saved: string | null | undefined, names: string[]): string {
  if (saved === ALL && names.length > 1) return ALL;
  if (saved && names.includes(saved)) return saved;
  return names.length > 0 ? names[0] : ALL;
}

/** The pick one step from `pick` (`1` next, `-1` previous), stopping at the ends. */
export function stepPick(names: string[], pick: string, dir: 1 | -1): string {
  const list = entries(names);
  const at = list.indexOf(pick);
  if (at < 0) return list[0];
  return list[Math.min(Math.max(at + dir, 0), list.length - 1)];
}

/** Whether there is a sheet after `pick`, and one before it (for the buttons). */
export function canStep(names: string[], pick: string) {
  const list = entries(names);
  const at = list.indexOf(pick);
  return { prev: at > 0, next: at >= 0 && at < list.length - 1 };
}

/** A canvas view: zoom and the offset of the canvas in the box. */
export interface ViewState {
  k: number;
  x: number;
  y: number;
}

/** The view a box shows when it fits `box` (in canvas pixels) in `width` by `height` screen pixels. */
export function fitView(
  width: number,
  height: number,
  box: { x: number; y: number; w: number; h: number },
  margin = 0.97,
): ViewState {
  const k = Math.min(width / box.w, height / box.h) * margin;
  return { k, x: (width - box.w * k) / 2 - box.x * k, y: (height - box.h * k) / 2 - box.y * k };
}

/** The view each thing had when it was left, kept for the session, by key. */
export class ViewMemo {
  private views = new Map<string, ViewState>();

  get(key: string): ViewState | undefined {
    return this.views.get(key);
  }

  set(key: string, view: ViewState) {
    this.views.set(key, view);
  }

  /** Forget one key. */
  drop(key: string) {
    this.views.delete(key);
  }

  /** Forget a key, or every key that starts with `prefix` (a board's views, say). */
  forget(prefix: string) {
    for (const k of [...this.views.keys()]) if (k.startsWith(prefix)) this.views.delete(k);
  }
}
