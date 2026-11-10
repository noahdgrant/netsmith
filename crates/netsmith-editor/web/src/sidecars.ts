// What the part inspector and the board's panel say of the two files beside a source file, read
// only: the pin map of a part (the pin file, ADR 0101) and the pick lock (ADR 0100). The server
// computes both; this turns them into the words and tones the panels show.

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
    };
  });
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
