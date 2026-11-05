// What the status bar, the strip above a stale view and the toasts say, as pure functions of
// plain values. The signals that feed them are in `state.ts`.

import type { Row } from './problems';

/** How long a time is, in a few characters: `4 s`, `1 min 5 s`. Nothing for a negative time. */
export function clock(ms: number): string {
  if (!(ms >= 0)) return '';
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  return `${m} min ${s % 60} s`;
}

/** Analysis that is done within this time is not announced: a spinner that flashes is noise. */
export const ANALYSING_SHOWN_AFTER_MS = 300;
/** The time is added to the label once the analysis has run this long. */
export const ANALYSING_CLOCK_AFTER_MS = 1000;

/** What the status bar says while the project is being analysed, or `null` when it says nothing
 * (idle, or not yet long enough to be worth announcing). */
export function analysingText(elapsedMs: number | null): string | null {
  if (elapsedMs === null || elapsedMs < ANALYSING_SHOWN_AFTER_MS) return null;
  return elapsedMs >= ANALYSING_CLOCK_AFTER_MS ? `Analysing… ${clock(elapsedMs)}` : 'Analysing…';
}

/** What is waiting for the server to analyse: a check of typed text, a saved file. */
export interface Pending {
  /** The id of the check last sent and not yet answered. */
  check: number | null;
  /** A file was written and the server's rebuild has not arrived. */
  save: boolean;
  /** When the analysis began, while there is any. */
  since: number | null;
}

export const IDLE: Pending = { check: null, save: false, since: null };

export type PendingEvent =
  | { type: 'check-sent'; id: number; now: number }
  | { type: 'checked'; id: number; latest: boolean }
  | { type: 'saved'; now: number }
  | { type: 'rebuilt' }
  | { type: 'lost' };

export const isBusy = (p: Pending) => p.check !== null || p.save;

/** What `p` becomes when something happens. The analysis is busy from the first request until
 * the answer to the last one arrives; the clock runs from the first. */
export function step(p: Pending, e: PendingEvent): Pending {
  let next: Pending;
  switch (e.type) {
    case 'check-sent':
      next = { ...p, check: e.id, since: p.since ?? e.now };
      break;
    case 'checked':
      // An answer to an earlier check says nothing about the text now on the page.
      next = e.latest || p.check === e.id ? { ...p, check: null } : p;
      break;
    case 'saved':
      next = { ...p, save: true, since: p.since ?? e.now };
      break;
    case 'rebuilt':
      next = { ...p, save: false };
      break;
    case 'lost':
      // A socket that closed will not answer.
      return IDLE;
  }
  return isBusy(next) ? next : IDLE;
}

export type Connection = 'connecting' | 'connected' | 'reconnecting' | 'offline';

/** A connection that comes back within this time is not worth saying anything about. */
export const RECONNECTING_AFTER_MS = 1500;
/** After this long without a connection the page says offline, not reconnecting. */
export const OFFLINE_AFTER_MS = 10_000;

/** What the page says about its socket. `lostAt` is when the socket closed (null while open or
 * never opened). A short loss is not reported, so the indicator does not flicker. */
export function connectionState(open: boolean, everOpen: boolean, lostAt: number | null, now: number): Connection {
  if (open) return 'connected';
  if (!everOpen) return now - (lostAt ?? now) >= OFFLINE_AFTER_MS ? 'offline' : 'connecting';
  const lost = lostAt === null ? 0 : now - lostAt;
  if (lost < RECONNECTING_AFTER_MS) return 'connected';
  return lost < OFFLINE_AFTER_MS ? 'reconnecting' : 'offline';
}

export const CONNECTION_LABEL: Record<Connection, string> = {
  connecting: 'Connecting…',
  connected: 'Live',
  reconnecting: 'Reconnecting…',
  offline: 'Offline',
};

export function connectionTip(c: Connection, lostMs: number | null): string {
  switch (c) {
    case 'connecting':
      return 'Connecting to the editor server.';
    case 'connected':
      return 'Connected to the editor server. Saved files and checks of typed text arrive live.';
    case 'reconnecting':
      return `The connection to the editor server was lost${lostMs ? ` ${clock(lostMs)} ago` : ''}; trying again every second. What you typed is kept in the page.`;
    case 'offline':
      return `The editor server has not answered for ${clock(lostMs ?? 0)}. Is \`netsmith editor\` still running? What you typed is kept in the page; saving needs the server.`;
  }
}

/** The strip above a view that does not show the current source. */
export interface Banner {
  kind: 'stale' | 'unsaved';
  chip: string;
  /** The first error's message. */
  text: string;
  /** `file:line`, when the error has a place. */
  where: string | null;
  /** How many more errors there are. */
  more: number;
  /** What is shown, in a sentence. */
  detail: string;
}

export interface BannerInput {
  /** The board on screen was built from an earlier text than the file on disk. */
  stale: boolean;
  /** The text being edited has errors the file on disk does not. */
  unsavedBroken: boolean;
  /** The errors of what is checked, in the dock's order. */
  errors: Row[];
}

export function bannerFor(i: BannerInput): Banner | null {
  if (!i.stale && !i.unsavedBroken) return null;
  const e = i.errors[0];
  const where = e?.at ? `${e.at.file}:${e.at.line}` : null;
  const common = {
    text: e?.message ?? 'The source has a mistake',
    where,
    more: Math.max(0, i.errors.length - 1),
  };
  return i.stale
    ? { ...common, kind: 'stale', chip: 'Showing last good version', detail: 'The source has an error, so this is the last version that built.' }
    : { ...common, kind: 'unsaved', chip: 'Unsaved edit has an error', detail: 'This shows the saved file; the text you are typing has an error.' };
}

/** The reason a job or a request failed, in one line: the first line of the output that says
 * `error`, else the first line that says anything. */
export function causeOf(output: string | null | undefined): string {
  const lines = (output ?? '')
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
  const err = lines.find((l) => /^(error|fatal|panic)\b/i.test(l) || /\berror\b[:\[]/i.test(l));
  return (err ?? lines[0] ?? '').slice(0, 300);
}
