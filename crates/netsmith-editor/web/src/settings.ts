// The page's settings besides the colour theme (theme.ts): how dense it is, how big, and whether
// it moves. The three are kept together in localStorage under `netsmith.settings` as one JSON
// object, and `index.html` applies them before the first paint with its own copy of `rootState`
// (a test runs that copy against this one), so the page does not open in one size and jump to
// another. Storage can be missing or throw; the page then has the defaults.
//
//   density  `comfortable` (the default) or `compact`: `[data-density]` on `<html>`; `themes.css`
//            gives the control, row, bar and gap tokens a smaller scale under `compact`
//   scale    90, 100, 110 or 125 percent: `--ui-scale` on `<html>`; the root font size is that
//            share of the browser's own, and the page's sizes are rem and tokens
//   motion   `system` (the operating system's choice stands) or `reduce` (nothing animates, whatever
//            the system says): `[data-motion]` on `<html>`

import { signal } from '@preact/signals';

export type Density = 'comfortable' | 'compact';
export type MotionPref = 'system' | 'reduce';
export const SCALES = [90, 100, 110, 125] as const;
export type Scale = (typeof SCALES)[number];

export interface Settings {
  density: Density;
  scale: Scale;
  motion: MotionPref;
}

export const SETTINGS_KEY = 'netsmith.settings';
export const DEFAULTS: Settings = { density: 'comfortable', scale: 100, motion: 'system' };

export const DENSITIES: { id: Density; label: string; hint: string }[] = [
  { id: 'comfortable', label: 'Comfortable', hint: 'Controls 28 px, rows 28 px: the default' },
  { id: 'compact', label: 'Compact', hint: 'Controls 24 px, rows 24 px, bars and gaps a step tighter: more on the screen' },
];
export const MOTIONS: { id: MotionPref; label: string; hint: string }[] = [
  { id: 'system', label: 'System', hint: 'Follows the operating system: reduced when it asks for it' },
  { id: 'reduce', label: 'Reduce', hint: 'Nothing slides, fades or glides, whatever the system asks' },
];

/** What is stored (anything, as JSON parsed) as settings: each field that is not one of its choices is its default. */
export function parseSettings(raw: unknown): Settings {
  const o = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  return {
    density: o.density === 'compact' ? 'compact' : 'comfortable',
    scale: (SCALES as readonly unknown[]).includes(o.scale) ? (o.scale as Scale) : 100,
    motion: o.motion === 'reduce' ? 'reduce' : 'system',
  };
}

/** What `<html>` carries for a set of settings: the two attributes and the scale as a number. `index.html` does the same. */
export function rootState(s: Settings): { density: Density; motion: MotionPref; uiScale: string } {
  return { density: s.density, motion: s.motion, uiScale: String(s.scale / 100) };
}

export function readSettings(storage: Pick<Storage, 'getItem'> | null = safeStorage()): Settings {
  try {
    const text = storage?.getItem(SETTINGS_KEY);
    return text == null ? { ...DEFAULTS } : parseSettings(JSON.parse(text));
  } catch {
    return { ...DEFAULTS };
  }
}

/** Whether it was kept. */
export function writeSettings(s: Settings, storage: Pick<Storage, 'setItem'> | null = safeStorage()): boolean {
  try {
    if (!storage) return false;
    storage.setItem(SETTINGS_KEY, JSON.stringify(s));
    return true;
  } catch {
    return false;
  }
}

function safeStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** Whether the page should not move: the person chose Reduce, or the system asks for it. */
export function reducedMotion(s: Settings = settings.peek(), system?: boolean): boolean {
  if (s.motion === 'reduce') return true;
  if (system !== undefined) return system;
  try {
    return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  } catch {
    return false;
  }
}

export const settings = signal<Settings>(readSettings());

function apply(root: HTMLElement = document.documentElement) {
  const r = rootState(settings.peek());
  root.dataset.density = r.density;
  root.dataset.motion = r.motion;
  root.style.setProperty('--ui-scale', r.uiScale);
}

/** What changing one setting says: the new state in words, and whether it will be remembered. */
export function settingMessage(label: string, value: string, kept: boolean): { text: string; bad: boolean } {
  return kept
    ? { text: `${label}: ${value}`, bad: false }
    : { text: `${label}: ${value}. It is not remembered: this browser does not keep settings.`, bad: true };
}

/** Change settings and keep them. Returns whether they were kept. */
export function changeSettings(patch: Partial<Settings>): boolean {
  settings.value = parseSettings({ ...settings.peek(), ...patch });
  const kept = writeSettings(settings.peek());
  apply();
  return kept;
}

export function resetSettings(): boolean {
  return changeSettings({ ...DEFAULTS });
}

let started = false;
/** Apply the stored settings, and follow another tab's change (once). */
export function startSettings() {
  settings.value = readSettings();
  apply();
  if (started) return;
  started = true;
  try {
    window.addEventListener('storage', (e) => {
      if (e.key !== SETTINGS_KEY) return;
      settings.value = readSettings();
      apply();
    });
  } catch {
    // No window events: the page keeps what it has.
  }
}
