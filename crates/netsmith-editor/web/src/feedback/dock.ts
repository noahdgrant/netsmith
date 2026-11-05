// The bottom dock's size: what it may be, and what is kept of it. Pure.

export const DOCK_MIN = 120;
export const DOCK_DEFAULT = 240;
/** The dock never takes more than this share of the window, so the views stay in reach. */
export const DOCK_MAX_SHARE = 0.7;

export interface DockPrefs {
  open: boolean;
  height: number;
}

export const clampDock = (height: number, windowHeight: number): number => {
  const max = Math.max(DOCK_MIN, Math.floor(windowHeight * DOCK_MAX_SHARE));
  const h = Number.isFinite(height) ? Math.round(height) : DOCK_DEFAULT;
  return Math.min(max, Math.max(DOCK_MIN, h));
};

/** What was kept, made safe: a hand-edited or old value falls back to the default. */
export function parseDock(raw: unknown): DockPrefs {
  const r = (raw ?? {}) as Partial<DockPrefs>;
  return {
    open: r.open === true,
    height: typeof r.height === 'number' && Number.isFinite(r.height) ? Math.max(DOCK_MIN, Math.round(r.height)) : DOCK_DEFAULT,
  };
}
