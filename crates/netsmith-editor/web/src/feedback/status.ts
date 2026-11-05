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

/** How far the server's analysis has got, as its `progress` messages say: the stage, what it is on,
 * and the count of what the stage works through. There is no total over all the stages: the server
 * does not know it before the checker has found the boards. */
export interface AnalysisProgress {
  stage: string;
  board: string;
  /** The index (from 0) of what the stage is on. */
  done: number;
  total: number;
}

const STAGE_VERB: Record<string, string> = {
  check: 'Checking',
  solve: 'Solving',
  lower: 'Lowering',
  draw: 'Drawing',
  modules: 'Checking module',
};

const STAGE_NOUN: Record<string, [string, string]> = {
  check: ['file', 'files'],
  solve: ['board', 'boards'],
  lower: ['board', 'boards'],
  draw: ['board', 'boards'],
  modules: ['module', 'modules'],
};

const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** `Solving Beta, 1 of 1`: the stage's own count and no more. */
export function progressText(p: AnalysisProgress): string {
  const verb = STAGE_VERB[p.stage] ?? capital(p.stage);
  const count = p.total > 0 ? `, ${Math.min(p.total, p.done + 1)} of ${p.total}` : '';
  return `${verb} ${p.board}${count}`;
}

/** What the status bar says while the project is being analysed, or `null` when it says nothing
 * (idle, or not yet long enough to be worth announcing). The stage the server is in follows the clock. */
export function analysingText(elapsedMs: number | null, progress: AnalysisProgress | null = null): string | null {
  if (elapsedMs === null || elapsedMs < ANALYSING_SHOWN_AFTER_MS) return null;
  const head = elapsedMs >= ANALYSING_CLOCK_AFTER_MS ? `Analysing… ${clock(elapsedMs)}` : 'Analysing…';
  return progress ? `${head} · ${progressText(progress)}` : head;
}

/** A stage the page has seen the server work through, for the tooltip after it. */
export interface StageSeen {
  stage: string;
  /** What it works through, as the last message said. */
  total: number;
  /** When the first message of the stage arrived, by the page's clock. */
  at: number;
}

/** What is waiting for the server to analyse: a check of typed text, a saved file, and an analysis
 * of the files on disk that the server announced (whoever's change began it). */
export interface Pending {
  /** The id of the check last sent and not yet answered. */
  check: number | null;
  /** A file was written and the server's rebuild has not arrived. */
  save: boolean;
  /** The analysis the server announced: `since` is its own start, in ms since the epoch, and
   * `progress` the last stage message. */
  server: { since: number; progress: AnalysisProgress | null; seen: StageSeen[] } | null;
  /** When the analysis began, while there is any: the server's start when it said, else the
   * page's first request. */
  since: number | null;
}

export const IDLE: Pending = { check: null, save: false, server: null, since: null };

export type PendingEvent =
  | { type: 'check-sent'; id: number; now: number }
  | { type: 'checked'; id: number; latest: boolean }
  | { type: 'saved'; now: number }
  | { type: 'analysing'; since: number }
  | ({ type: 'progress'; now: number } & AnalysisProgress)
  | { type: 'rebuilt' }
  | { type: 'lost' };

export const isBusy = (p: Pending) => p.check !== null || p.save || p.server !== null;

/** What `p` becomes when something happens. The analysis is busy from the first request (or the
 * server's announcement) until the answer to the last one arrives; the clock runs from the first,
 * and from the server's own start where it said when that was. */
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
    case 'analysing':
      // The same start again (a page that connects mid-way is told it once) keeps what was seen.
      next = {
        ...p,
        server: p.server?.since === e.since ? p.server : { since: e.since, progress: null, seen: [] },
        since: e.since,
      };
      break;
    case 'progress': {
      const server = p.server ?? { since: p.since ?? e.now, progress: null, seen: [] };
      const progress = { stage: e.stage, board: e.board, done: e.done, total: e.total };
      const known = server.seen.find((x) => x.stage === e.stage);
      const seen = known
        ? server.seen.map((x) => (x === known ? { ...x, total: e.total } : x))
        : [...server.seen, { stage: e.stage, total: e.total, at: e.now }];
      next = { ...p, server: { ...server, progress, seen }, since: server.since };
      break;
    }
    case 'rebuilt':
      next = { ...p, save: false, server: null };
      break;
    case 'lost':
      // A socket that closed will not answer.
      return IDLE;
  }
  return isBusy(next) ? next : IDLE;
}

/** An analysis that has ended, for the status bar's `Analysed in 46 s` and its tooltip. */
export interface Analysed {
  /** When it ended, by the page's clock. */
  at: number;
  /** How long it took, in ms: the server's own figure where it sent one. */
  took: number;
  /** The stages the page saw it work through, with the time in each. */
  stages: string[];
  /** It ended while the page was looking (the calm line is for these, not for an analysis read at load). */
  seen: boolean;
}

/** One line for each stage the page saw: `Solve (1 board): 31 s`. The time of a stage is until the
 * next one's first message (the last, until `endedAt`). */
export function stageLines(seen: StageSeen[], endedAt: number): string[] {
  return seen.map((s, i) => {
    const [one, many] = STAGE_NOUN[s.stage] ?? ['item', 'items'];
    const until = seen[i + 1]?.at ?? endedAt;
    return `${capital(s.stage)} (${s.total} ${s.total === 1 ? one : many}): ${clock(Math.max(0, until - s.at))}`;
  });
}

/** How long the calm `Analysed in 46 s` stays after an analysis ends. */
export const ANALYSED_SHOWN_MS = 12_000;

/** `Analysed in 46 s` for a while after one that took a second or more; plain `Analysed` otherwise. */
export function analysedText(a: Analysed | null, now: number): string {
  if (!a || !a.seen || a.took < 1000 || now - a.at > ANALYSED_SHOWN_MS) return 'Analysed';
  return `Analysed in ${clock(a.took)}`;
}

/** The tooltip of the idle item. */
export function analysedTip(a: Analysed | null): string {
  if (!a) return 'Analysis is up to date.';
  const lines = a.stages.length ? `\n${a.stages.join('\n')}` : '';
  return `Analysis is up to date. The last one took ${clock(a.took)}.${lines}`;
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
