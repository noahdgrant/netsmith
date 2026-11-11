// What the part inspector and the board's panel say of the two files beside a source file: the
// pin map of a part (the pin file, ADR 0101) and the pick lock (ADR 0100). The server computes
// both; this turns them into the words and tones the panels show. The pin map has a padlock and
// a move for each request, which write the pin file's `lock`, `pin` and `source` lines; the
// pick lock is read only.

import type { PickLockInfo, PinRequestInfo } from './api';

export interface PinRow {
  request: string;
  /** The pin's path in the part (`p00`), or `''` for a request that got none. */
  pin: string;
  /** `pad 3`, or `''` when the design has no pad for the pin. */
  pad: string;
  function: string;
  locked: boolean;
  /** Where the pin came from, in words; `''` for a request with no pin. */
  origin: string;
  /** What `origin` means, for a tooltip. */
  originTitle: string;
  /** The check's message for a request that has no pin. */
  message: string;
  /** The pins the request may be moved to, each with whether it is free: what the move list and the drop targets are. */
  targets: PinTarget[];
}

/** A pin a request may be moved to. `free` is a pin no other request has; the one the request has now is `current`. */
export interface PinTarget {
  pin: string;
  /** `pad 6`, or `''` when the design has none. */
  pad: string;
  state: 'current' | 'free' | 'held';
  /** What the pin is, in words, for the list's tooltip. */
  title: string;
  /** The button's name for a screen reader: the pin, its pad and what it is to the request. */
  label: string;
}

const ORIGIN: Record<string, [string, string]> = {
  file: ['file', 'The pin file beside the source holds this entry, and it still holds'],
  run: ['picked now', 'Picked from the source by this reading: the pin file has no entry for it yet (build writes one)'],
};

/** One row for each request of a part, as the server listed them. */
export function pinRows(requests: PinRequestInfo[] | undefined): PinRow[] {
  return (requests ?? []).map((r) => {
    const [word, title] = ORIGIN[r.from ?? ''] ?? ['', ''];
    return {
      request: r.request,
      pin: r.pin ?? '',
      pad: r.pad ? `pad ${r.pad}` : '',
      function: r.function,
      locked: r.locked,
      origin: word && r.from === 'file' && r.entry ? `${word}, ${r.entry}` : word,
      originTitle: title,
      message: r.message ?? '',
      targets: targetsOf(r),
    };
  });
}

/** The pins a request may be moved to, in the order the server lists them (the byte order of their paths). */
export function targetsOf(r: PinRequestInfo): PinTarget[] {
  return (r.offers ?? []).map((o) => {
    const pad = o.pad ? `pad ${o.pad}` : '';
    const state = o.pin === r.pin ? 'current' : o.heldBy ? 'held' : 'free';
    const title =
      state === 'current'
        ? `${o.pin} is where \`${r.request}\` is now`
        : state === 'held'
          ? `${o.pin} is held by \`${o.heldBy}\`: the editor refuses this move`
          : `Move \`${r.request}\` to ${o.pin}${pad ? ` (${pad})` : ''}`;
    const label = `${o.pin}${pad ? `, ${pad}` : ''}${state === 'current' ? ', where it is now' : state === 'held' ? `, held by ${o.heldBy}` : ''}`;
    return { pin: o.pin, pad, state, title, label };
  });
}

/** The pin file as the project names it, from the board's source file: `src/main.nsm` has `src/main.pins.toml`. */
export const pinFileOf = (source: string): string => source.replace(/\.nsm$/, '') + '.pins.toml';

/** What the padlock of a request says, as its tooltip: the state, what a run does with it, and what a click writes. */
export function padlockTitle(r: Pick<PinRow, 'request' | 'locked'>, file: string): string {
  return r.locked
    ? `Locked: a run never moves this pin. Click to unlock it (removes \`lock = true\` from ${file}).`
    : `Unlocked: a run may move this pin to make the board route. Click to lock it (writes \`lock = true\` in ${file}).`;
}

/** Why a request cannot be moved by hand, or null when it can: a locked request stays where it is. */
export function cannotMove(r: Pick<PinRow, 'request' | 'locked' | 'message' | 'targets'>): string | null {
  if (r.message) return 'This request has no pin yet.';
  if (r.locked) return `Unlock \`${r.request}\` to move it.`;
  if (r.targets.filter((t) => t.state !== 'current').length === 0) return 'No other pin offers this function.';
  return null;
}

/** How many requests of a part have no pin. */
export const withoutPin = (rows: PinRow[]): number => rows.filter((r) => r.message !== '').length;

export interface LockView {
  /** The one word for the section's count: `current`, `not current`, `absent`, `unreadable`. */
  state: string;
  /** The tone of that word. */
  tone: 'ok' | 'warn' | 'bad';
  /** A sentence that says what it means and what writes it. */
  says: string;
  /** What `netsmith lock --check` prints for it, one line each. */
  lines: string[];
}

/** What the pick lock of the board's source file is, in words. */
export function lockView(lock: PickLockInfo): LockView {
  if (lock.error) return { state: 'unreadable', tone: 'bad', says: lock.error, lines: [] };
  if (!lock.exists && lock.current) return { state: 'none needed', tone: 'ok', says: 'The source has no part to lock.', lines: [] };
  if (!lock.exists) {
    return { state: 'absent', tone: 'warn', says: 'No lock file yet: `netsmith build` writes it, or `netsmith lock --update`.', lines: [] };
  }
  if (lock.current) return { state: 'current', tone: 'ok', says: 'Every part is held to the row the solver picks now.', lines: [] };
  return {
    state: 'not current',
    tone: 'warn',
    says: 'The solver now picks other rows than the file holds. `netsmith lock --update` writes them.',
    lines: lock.diff,
  };
}
