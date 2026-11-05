// The command registry. A command exists once: the palette, the cheat sheet, the buttons and
// the keyboard all read it. A command has an id, a title, a group, an optional key chord, what it
// does and when it can be done. Nothing here knows the page (the commands that act on the store
// are listed in `commandset.ts`), so it can be tested alone.

export interface Command {
  /** Stable, dotted: `view.power`. Recently used commands are remembered by it. */
  id: string;
  title: string;
  /** What the cheat sheet and the palette group it under: `View`, `Go to`, `Files`. */
  group: string;
  /** The chord that runs it from anywhere, such as `Mod+Shift+P`, `Alt+1`, `?`. */
  keys?: string;
  /** Further chords that run the same command (shown after the first in the cheat sheet). */
  alsoKeys?: string[];
  /** Words the palette also matches (`kicad`, `export`). */
  aliases?: string[];
  run: () => void | Promise<void>;
  /** Whether it can be done now; absent means always. A command that cannot is not offered. */
  when?: () => boolean;
  /** A chord that works while typing in a field or the source editor (one with Ctrl, Alt or
   * Cmd always does; a plain key such as `?` or `1` does not). */
  inFields?: boolean;
}

const commands = new Map<string, Command>();
const providers: (() => Command[])[] = [];

/** Add commands. A command with an id that is there already replaces it. */
export function register(...list: Command[]) {
  for (const c of list) commands.set(c.id, c);
}

/** Add a source of commands that depend on the page (one to switch to each board). */
export function provide(fn: () => Command[]) {
  providers.push(fn);
}

/** Forget everything (for tests). */
export function clearRegistry() {
  commands.clear();
  providers.length = 0;
}

/** Every command that can be done now, in the order they were added. */
export function available(): Command[] {
  const all = [...commands.values(), ...providers.flatMap((p) => p())];
  return all.filter((c) => {
    try {
      return !c.when || c.when();
    } catch {
      return false;
    }
  });
}

/** Every command, whether or not it can be done now (the cheat sheet lists them all). */
export function allCommands(): Command[] {
  return [...commands.values(), ...providers.flatMap((p) => p())];
}

export function find(id: string): Command | undefined {
  return allCommands().find((c) => c.id === id);
}

/** Run a command by id if it can be done now. Returns whether it ran. */
export function runCommand(id: string): boolean {
  const c = find(id);
  if (!c || (c.when && !c.when())) return false;
  void Promise.resolve(c.run()).catch(() => undefined);
  return true;
}

// ---- chords ----

export interface Chord {
  mod: boolean;
  alt: boolean;
  shift: boolean;
  /** Upper case for a letter, as `KeyboardEvent.key` otherwise (`ArrowLeft`, `\`, `?`, `F12`). */
  key: string;
}

const NAMED: Record<string, string> = { esc: 'Escape', escape: 'Escape', left: 'ArrowLeft', right: 'ArrowRight', up: 'ArrowUp', down: 'ArrowDown', space: ' ', enter: 'Enter', return: 'Enter' };

/** `Mod+Shift+P` as a chord. `Mod` is Ctrl, or Cmd on a Mac. */
export function parseChord(text: string): Chord {
  const parts = text.split('+');
  // A chord that ends with `+` (the key itself) leaves an empty last piece.
  let key = parts.pop() ?? '';
  if (key === '' && parts.length > 0 && text.endsWith('++')) key = '+';
  const mods = new Set(parts.map((p) => p.toLowerCase()));
  const lower = key.toLowerCase();
  const named = NAMED[lower];
  return {
    mod: mods.has('mod') || mods.has('ctrl') || mods.has('cmd'),
    alt: mods.has('alt'),
    shift: mods.has('shift'),
    key: named ?? (key.length === 1 ? key.toUpperCase() : key),
  };
}

/** The chord an event is. A shifted symbol (`?`) is its own key and carries no Shift. */
export function chordOfEvent(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'code'>): Chord {
  let key = e.key;
  // Alt changes the character on a Mac (Alt+1 is ¡): the physical key tells which was meant.
  if (e.altKey && key.length === 1 && !/[a-z0-9]/i.test(key) && e.code) {
    const digit = /^Digit(\d)$/.exec(e.code);
    const letter = /^Key([A-Z])$/.exec(e.code);
    if (digit) key = digit[1];
    else if (letter) key = letter[1];
  }
  const single = key.length === 1;
  const letter = single && key.toLowerCase() !== key.toUpperCase();
  return {
    mod: e.ctrlKey || e.metaKey,
    alt: e.altKey,
    // Shift is part of a chord for letters, digits and named keys; for a symbol it is in the character.
    shift: e.shiftKey && (!single || letter || /\d/.test(key)),
    key: single ? key.toUpperCase() : key,
  };
}

export const sameChord = (a: Chord, b: Chord) => a.mod === b.mod && a.alt === b.alt && a.shift === b.shift && a.key === b.key;

/** The commands whose chord this event is, which can be done now. */
export function commandsForEvent(e: Parameters<typeof chordOfEvent>[0]): Command[] {
  const chord = chordOfEvent(e);
  return available().filter((c) => [c.keys, ...(c.alsoKeys ?? [])].some((k) => k && sameChord(parseChord(k), chord)));
}

const isMac = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent || '');

const KEY_NAMES: Record<string, string> = { ArrowLeft: '←', ArrowRight: '→', ArrowUp: '↑', ArrowDown: '↓', Escape: 'Esc', ' ': 'Space', Enter: 'Enter' };

/** The pieces of a chord for display: `['Ctrl', 'Shift', 'P']`, or with symbols on a Mac. */
export function chordLabels(text: string, mac = isMac()): string[] {
  const c = parseChord(text);
  const out: string[] = [];
  if (mac) {
    if (c.alt) out.push('⌥');
    if (c.shift) out.push('⇧');
    if (c.mod) out.push('⌘');
  } else {
    if (c.mod) out.push('Ctrl');
    if (c.alt) out.push('Alt');
    if (c.shift) out.push('Shift');
  }
  out.push(KEY_NAMES[c.key] ?? c.key);
  return out;
}

/** A chord as one string for a tooltip: `Ctrl+Shift+P`. */
export function chordText(text: string, mac = isMac()): string {
  return chordLabels(text, mac).join(mac ? '' : '+');
}

/** The tooltip text of something a command does: `Split right (Ctrl+\)`. */
export function withKey(label: string, id: string): string {
  const k = find(id)?.keys;
  return k ? `${label} (${chordText(k)})` : label;
}

/** Two commands with the same chord: a mistake in the list, found by a test. */
export function chordClashes(list: Command[] = allCommands()): string[] {
  const seen = new Map<string, string>();
  const clashes: string[] = [];
  for (const c of list) {
    for (const k of [c.keys, ...(c.alsoKeys ?? [])]) {
      if (!k) continue;
      const norm = JSON.stringify(parseChord(k));
      const other = seen.get(norm);
      if (other && other !== c.id) clashes.push(`${k}: ${other} and ${c.id}`);
      seen.set(norm, c.id);
    }
  }
  return clashes;
}

// ---- recently used ----

const RECENT_MAX = 12;

/** `id` moved to the front of the list of recently used ids. */
export function touchRecent(recent: string[], id: string): string[] {
  return [id, ...recent.filter((x) => x !== id)].slice(0, RECENT_MAX);
}

/** Put the commands used recently first, most recent first; the others keep their order. */
export function byRecent<T extends { id: string }>(list: T[], recent: string[]): T[] {
  const rank = new Map(recent.map((id, i) => [id, i]));
  const used = list.filter((c) => rank.has(c.id)).sort((a, b) => rank.get(a.id)! - rank.get(b.id)!);
  return [...used, ...list.filter((c) => !rank.has(c.id))];
}
