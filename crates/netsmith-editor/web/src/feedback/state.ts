// What the page keeps about its own feedback: the dock's size and filter, what is being analysed,
// the state of the socket, and the toasts. Nothing here imports `store.ts`, so the store can feed
// it without a cycle; what needs both is in `derive.ts`.

import { computed, effect, signal } from '@preact/signals';

import { noteError } from '../diagnostics';
import { load, save } from '../storage';
import { DOCK_DEFAULT, type DockPrefs, clampDock, parseDock } from './dock';
import { DEFAULT_FILTER, type Filter, type Group, parseGroup } from './problems';
import {
  type Analysed,
  type Connection,
  IDLE,
  type Pending,
  type PendingEvent,
  connectionState,
  isBusy,
  stageLines,
  step,
} from './status';

// ---- the dock

const DOCK_KEY = 'netsmith.dock.v1';
const FILTER_KEY = 'netsmith.dock.filter.v1';

/** Whether the dock is open and how tall: kept, so the page comes back as it was left. */
export const dock = signal<DockPrefs>(parseDock(load<unknown>(DOCK_KEY, null)));
effect(() => save(DOCK_KEY, dock.value));

export const filter = signal<Filter>({ ...DEFAULT_FILTER, ...load<Partial<Filter>>(FILTER_KEY, {}), text: '' });
export const group = signal<Group>(parseGroup(load<unknown>(`${FILTER_KEY}.group`, 'cause')));
effect(() => {
  const { text: _text, ...kept } = filter.value;
  save(FILTER_KEY, kept);
});
effect(() => save(`${FILTER_KEY}.group`, group.value));

/** The groups of the dock that are closed, by `<grouping>:<key>` (a grouping's own keys: a cause, a file, a severity, a stage). Not kept: a reload opens them all. */
export const closedGroups = signal<string[]>([]);
export const groupId = (by: Group, key: string) => `${by}:${key}`;
export function toggleGroup(by: Group, key: string) {
  const id = groupId(by, key);
  closedGroups.value = closedGroups.peek().includes(id) ? closedGroups.peek().filter((g) => g !== id) : [...closedGroups.peek(), id];
}

export function setDockOpen(open: boolean) {
  if (dock.peek().open !== open) dock.value = { ...dock.peek(), open };
}

export const toggleDock = () => setDockOpen(!dock.peek().open);

export function setDockHeight(height: number) {
  dock.value = { ...dock.peek(), height: clampDock(height, window.innerHeight) };
}

export const resetDockHeight = () => setDockHeight(DOCK_DEFAULT);

/** The row picked in the dock, by its id. */
export const activeRow = signal<string | null>(null);

// ---- analysis

export const pending = signal<Pending>(IDLE);
export const busy = computed(() => isBusy(pending.value));

/** The analysis that last ended, for the status bar's `Analysed in 46 s` and its tooltip. */
export const lastAnalysis = signal<Analysed | null>(null);

/** The page has asked for the first analysis and the server said it is still running (`503` with
 * `pending`): when the page was told and how long it had run by then. The views show their skeleton
 * and the time until `rebuilt` brings the analysis. */
export const booting = signal<{ at: number; elapsedMs: number } | null>(null);

/** When the analysis that the page waits for began, by the server's clock and else by what it said
 * of its age: for the clock the skeleton shows. */
export const analysisSince = computed<number | null>(() => {
  const s = pending.value.server?.since;
  if (s !== undefined) return s;
  const b = booting.value;
  return b ? b.at - b.elapsedMs : null;
});

export function track(e: PendingEvent, elapsedMs?: number) {
  const before = pending.peek();
  const after = step(before, e);
  pending.value = after;
  if (e.type === 'rebuilt') booting.value = null;
  if (isBusy(before) && !isBusy(after) && before.since !== null && e.type !== 'lost') {
    const at = Date.now();
    lastAnalysis.value = {
      at,
      // The server's figure is of the analysis alone: the page's clock also has the request.
      took: elapsedMs ?? at - before.since,
      stages: before.server ? stageLines(before.server.seen, at) : [],
      seen: true,
    };
  } else if (e.type === 'rebuilt' && elapsedMs !== undefined && !isBusy(before)) {
    // The announcement was missed (the socket opened after it): the server's figure still stands.
    lastAnalysis.value = { at: Date.now(), took: elapsedMs, stages: [], seen: true };
  }
}

/** What `GET /api/analysis` says of the analysis the page read at load: the tooltip has it. */
export function noteElapsed(elapsedMs: number | undefined) {
  if (elapsedMs !== undefined && !lastAnalysis.peek()) lastAnalysis.value = { at: 0, took: elapsedMs, stages: [], seen: false };
}

/** The time, as the page last looked: a signal that ticks only while something shows a clock. */
export const now = signal(Date.now());
let tickers = 0;
let ticker: ReturnType<typeof setInterval> | undefined;

/** Keep `now` current while a component that shows a clock is on screen. Returns what stops it. */
export function runClock(): () => void {
  if (tickers++ === 0) ticker = setInterval(() => (now.value = Date.now()), 250);
  now.value = Date.now();
  return () => {
    if (--tickers === 0) clearInterval(ticker);
  };
}

// ---- the socket

export const socketOpen = signal(false);
const everOpen = signal(false);
const lostAt = signal<number | null>(null);
/** The page's view of its connection to the server: it does not flicker for a short loss. */
export const connection = computed<Connection>(() =>
  connectionState(socketOpen.value, everOpen.value, lostAt.value, now.value),
);
export const lostFor = computed(() => (lostAt.value === null ? null : now.value - lostAt.value));

let watch: ReturnType<typeof setInterval> | undefined;

export function setSocket(open: boolean) {
  socketOpen.value = open;
  if (open) {
    everOpen.value = true;
    lostAt.value = null;
    clearInterval(watch);
    watch = undefined;
  } else {
    track({ type: 'lost' });
    lostAt.value ??= Date.now();
    // While there is no socket, the clock runs so the state can change from "live" to "reconnecting".
    watch ??= setInterval(() => (now.value = Date.now()), 500);
  }
}

// ---- toasts

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastItem {
  id: number;
  kind: 'ok' | 'bad' | 'info';
  title: string;
  /** What more there is to say: the cause of a failure. */
  detail?: string;
  actions: ToastAction[];
  /** A toast that is not dismissed by the time: a failure stays until it is read. */
  sticky: boolean;
}

export const toasts = signal<ToastItem[]>([]);
const MAX_TOASTS = 4;
const SHOWN_MS = 6000;
let nextToast = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function dismissToast(id: number) {
  clearTimeout(timers.get(id));
  timers.delete(id);
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

function arm(id: number, ms = SHOWN_MS) {
  clearTimeout(timers.get(id));
  timers.set(id, setTimeout(() => dismissToast(id), ms));
}

/** Pointer over a toast: its time stops. */
export function holdToast(id: number) {
  clearTimeout(timers.get(id));
  timers.delete(id);
}

/** Pointer gone: it has a little time still. */
export function releaseToast(id: number) {
  const t = toasts.peek().find((x) => x.id === id);
  if (t && !t.sticky) arm(id, 2500);
}

export interface Notice {
  kind?: ToastItem['kind'];
  title: string;
  detail?: string;
  actions?: ToastAction[];
  sticky?: boolean;
  /** How long it stays, when it is not sticky. */
  ms?: number;
}

/** Show a toast. The same words again replace the toast that has them, so a repeated action does
 * not pile them up. */
export function notify(n: Notice): number {
  const kind = n.kind ?? 'info';
  if (kind === 'bad') noteError(`${n.title}${n.detail ? `: ${n.detail}` : ''}`);
  const same = toasts.peek().find((t) => t.title === n.title && t.kind === kind && t.detail === n.detail);
  const id = same?.id ?? nextToast++;
  const item: ToastItem = {
    id,
    kind,
    title: n.title,
    detail: n.detail,
    actions: n.actions ?? [],
    sticky: n.sticky ?? (kind === 'bad' && (n.actions?.length ?? 0) > 0),
  };
  const rest = toasts.peek().filter((t) => t.id !== id);
  toasts.value = [...rest, item].slice(-MAX_TOASTS);
  if (item.sticky) clearTimeout(timers.get(id));
  else arm(id, n.ms ?? (kind === 'bad' ? SHOWN_MS * 1.5 : SHOWN_MS));
  return id;
}
