// The colour theme: what the person chose (a preference), what that comes to on this machine (the
// resolved theme, the one `<html data-theme>` names), and the signal the page reads.
//
// The choice is kept in localStorage under `netsmith.theme` as a JSON string, and `index.html`
// reads it before the first paint with its own copy of `resolveTheme` (a test runs that copy
// against this one), so the page does not flash the wrong colours. Storage can be missing or
// throw; the page then follows the system.

import { signal } from '@preact/signals';

/** What the person picks. */
export type ThemePref = 'system' | 'light' | 'dark' | 'hc-dark' | 'hc-light';
/** What the page is drawn in: the names in the `[data-theme]` selectors of `themes.css`. */
export type Theme = 'light' | 'dark' | 'hc-dark' | 'hc-light';

export const THEME_KEY = 'netsmith.theme';

export const THEMES: { id: ThemePref; label: string; hint: string }[] = [
  { id: 'system', label: 'System', hint: 'Follows the operating system: light or dark, and high contrast when it asks for it' },
  { id: 'light', label: 'Light', hint: 'Light chrome; the schematic on cream paper' },
  { id: 'dark', label: 'Dark', hint: 'The default dark theme' },
  { id: 'hc-light', label: 'High contrast light', hint: 'White, black borders, every text at least 7:1' },
  { id: 'hc-dark', label: 'High contrast dark', hint: 'Black, white borders, every text at least 7:1' },
];

/** What the machine asks for. */
export interface Environment {
  dark: boolean;
  /** `prefers-contrast: more`. */
  contrast: boolean;
}

/** A stored value as a preference; anything that is not one is `system`. */
export function parsePref(raw: unknown): ThemePref {
  return THEMES.some((t) => t.id === raw) ? (raw as ThemePref) : 'system';
}

/** The theme a preference comes to under `env`. */
export function resolveTheme(pref: ThemePref, env: Environment): Theme {
  if (pref !== 'system') return pref;
  if (env.contrast) return env.dark ? 'hc-dark' : 'hc-light';
  return env.dark ? 'dark' : 'light';
}

/** Whether a theme is a dark one (the editor's own colouring and the browser's widgets need it). */
export function isDark(theme: Theme): boolean {
  return theme === 'dark' || theme === 'hc-dark';
}

export function readPref(storage: Pick<Storage, 'getItem'> | null = safeStorage()): ThemePref {
  try {
    const text = storage?.getItem(THEME_KEY);
    return text == null ? 'system' : parsePref(JSON.parse(text));
  } catch {
    return 'system';
  }
}

/** Whether it was kept. */
export function writePref(pref: ThemePref, storage: Pick<Storage, 'setItem'> | null = safeStorage()): boolean {
  try {
    if (!storage) return false;
    storage.setItem(THEME_KEY, JSON.stringify(pref));
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

function environment(): Environment {
  try {
    return {
      dark: window.matchMedia('(prefers-color-scheme: dark)').matches,
      contrast: window.matchMedia('(prefers-contrast: more)').matches,
    };
  } catch {
    return { dark: true, contrast: false };
  }
}

/** The person's choice. */
export const themePref = signal<ThemePref>(readPref());
/** What is drawn. */
export const theme = signal<Theme>(resolveTheme(themePref.peek(), environment()));

function apply() {
  const next = resolveTheme(themePref.peek(), environment());
  theme.value = next;
  const root = document.documentElement;
  root.dataset.theme = next;
  root.style.colorScheme = isDark(next) ? 'dark' : 'light';
}

/** Choose a theme and keep the choice. Returns whether the choice was kept. */
export function setTheme(pref: ThemePref): boolean {
  themePref.value = pref;
  const kept = writePref(pref);
  apply();
  return kept;
}

let watching = false;
/** Apply the stored choice and follow the system while it is `system` (once). */
export function startTheme() {
  apply();
  if (watching) return;
  watching = true;
  try {
    for (const q of ['(prefers-color-scheme: dark)', '(prefers-contrast: more)']) {
      window.matchMedia(q).addEventListener('change', () => themePref.peek() === 'system' && apply());
    }
    // Another tab changed it.
    window.addEventListener('storage', (e) => {
      if (e.key !== THEME_KEY) return;
      themePref.value = readPref();
      apply();
    });
  } catch {
    // No media queries: the page keeps what it has.
  }
}

/** What the person is told after choosing: which theme it came to, and whether it will be remembered. */
export function themeMessage(pref: ThemePref, resolved: Theme, kept: boolean): { text: string; bad: boolean } {
  const label = THEMES.find((t) => t.id === pref)?.label ?? pref;
  const now = pref === 'system' ? `System: ${THEMES.find((t) => t.id === resolved)?.label ?? resolved} now` : label;
  return kept ? { text: `Theme: ${now}`, bad: false } : { text: `Theme: ${now}. It is not remembered: this browser does not keep settings.`, bad: true };
}
