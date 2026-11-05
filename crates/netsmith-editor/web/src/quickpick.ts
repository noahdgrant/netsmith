// The state of go to anything and the command palette (one input, two sets of rows), and of the
// keyboard cheat sheet: whether they are open, what was typed last, and what was chosen lately.
// The window itself is `components/QuickPick.tsx`.

import { signal } from '@preact/signals';

import { touchRecent } from './commands';
import { load, save } from './storage';

/** Open when not null. `initial` is what the input starts with, `nonce` makes opening twice a new open. */
export const picker = signal<{ initial: string; nonce: number } | null>(null);
export const cheatSheet = signal(false);
/** The group of the cheat sheet to scroll to when it opens (`Schematic`), or null. */
export const cheatGroup = signal<string | null>(null);

const RECENT_COMMANDS = 'netsmith.recentCommands.v1';
const RECENT_PICKS = 'netsmith.recentPicks.v1';
const LAST_QUERY = 'netsmith.lastQuery.v1';

export const recentCommands = signal<string[]>(load<string[]>(RECENT_COMMANDS, []));
export const recentPicks = signal<string[]>(load<string[]>(RECENT_PICKS, []));

/** What was typed the last time go to anything closed, for this session (the page's tab). */
function loadQuery(): string {
  try {
    return window.sessionStorage.getItem(LAST_QUERY) ?? '';
  } catch {
    return '';
  }
}
let lastQuery = typeof window === 'undefined' ? '' : loadQuery();

export function rememberQuery(q: string) {
  // The command palette starts empty each time; only a search is kept.
  if (q.startsWith('>')) return;
  lastQuery = q;
  try {
    window.sessionStorage.setItem(LAST_QUERY, q);
  } catch {
    // Not kept; the session goes on without it.
  }
}

export function usedCommand(id: string) {
  recentCommands.value = touchRecent(recentCommands.value, id);
  save(RECENT_COMMANDS, recentCommands.value);
}

export function usedPick(id: string) {
  recentPicks.value = touchRecent(recentPicks.value, id);
  save(RECENT_PICKS, recentPicks.value);
}

let opens = 0;

/** Open the input. `prefix` is a mode (`>`, `@`, `#`, `:`); without one it starts with the last
 * search of the session. */
export function openPicker(prefix?: string) {
  cheatSheet.value = false;
  picker.value = { initial: prefix ?? lastQuery, nonce: ++opens };
}

export function closePicker() {
  picker.value = null;
}

/** Show or hide the cheat sheet; `group` is the part of it to scroll to when it opens. */
export function toggleCheatSheet(open = !cheatSheet.peek(), group: string | null = null) {
  picker.value = null;
  cheatGroup.value = open ? group : null;
  cheatSheet.value = open;
}
