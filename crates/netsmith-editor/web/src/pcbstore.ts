// What the PCB canvas knows: the scene of the board, the selection, the tool, and the history
// of edits. An edit goes to the server, which writes it to the layout file and answers with
// the board placed again; the page never keeps a second copy of the layout.

import { signal } from '@preact/signals';

import type { PcbChange, PcbFindings, PcbScene, RoutesChange } from './api';
import { ApiFailure, api } from './client';
import { type History, NO_HISTORY, type Step, record, redoing, undoing } from './pcb/edits';
import { type Selection, partSel, sameSelection, selection as shared, setSelection } from './selection';
import { beginJob, endJob, pcbReports, say } from './store';

export const scene = signal<PcbScene | null>(null);
/** Why the board could not be placed, if it could not. */
export const sceneError = signal<string | null>(null);
/** The parts picked, by their path in the design. */
export const selection = signal<string[]>([]);
/** The keepout picked, by name. */
export const pickedKeepout = signal<string | null>(null);
/** The record of the routes file picked: a net's record, or one shape of it, numbered from 1 as
 * `netsmith routes show` numbers them. */
export interface RoutePick {
  net: string;
  track?: number;
  via?: number;
}
export const pickedRoute = signal<RoutePick | null>(null);
export type Tool = 'select' | 'keepout';
export const tool = signal<Tool>('select');
export const history = signal<History>(NO_HISTORY);
/** What is wrong with the board as it is now, and the finding picked in the list. */
export const findings = signal<PcbFindings | null>(null);
export const pickedFinding = signal<string | null>(null);
/** Whether KiCad's check is running. */
export const checking = signal(false);

/** The grids the canvas offers, in mm; 0 is none. */
export const GRIDS = [0, 0.05, 0.1, 0.25, 0.5, 1, 1.27, 2.54];
const GRID_KEY = 'netsmith.pcb.grid';

function loadGrid(): number {
  try {
    const v = Number(localStorage.getItem(GRID_KEY));
    return GRIDS.includes(v) && localStorage.getItem(GRID_KEY) !== null ? v : 0.5;
  } catch {
    return 0.5;
  }
}

/** The grid parts snap to while they move. A setting of the page, kept in the browser. */
export const grid = signal(loadGrid());

export function setGrid(mm: number) {
  grid.value = mm;
  try {
    localStorage.setItem(GRID_KEY, String(mm));
  } catch {
    // A browser that keeps nothing keeps the grid for the session.
  }
}

/** What a new keepout forbids and where. */
export const keepoutDefaults = signal({ layers: 'all', tracks: true, vias: true, copper: true, parts: true });

// Edits and loads one after the other, so the stamp each carries is the one the last answered.
let queue: Promise<unknown> = Promise.resolve();
function enqueue<T>(job: () => Promise<T>): Promise<T> {
  const next = queue.then(job, job);
  queue = next.catch(() => undefined);
  return next;
}

/** Take the scene of a board from the server. The selection keeps what is still there. */
export function loadScene(board: string): Promise<void> {
  return enqueue(async () => {
    try {
      const next = await api.pcbScene(board);
      show(next);
      sceneError.value = null;
    } catch (e) {
      scene.value = null;
      sceneError.value = e instanceof Error ? e.message : String(e);
    }
  });
}

let findingsAsked = 0;

/** Ask the server what is wrong with the board as it is; the answer of the last ask is kept. */
export async function loadFindings(board: string): Promise<void> {
  const mine = ++findingsAsked;
  try {
    const answer = await api.pcbFindings(board);
    if (mine === findingsAsked) findings.value = answer;
  } catch {
    // The scene's own load says why the board cannot be placed.
  }
}

/** Ask KiCad's design rules check about the board as well (a few seconds); netsmith's own check needs no asking. */
export async function recheck(board: string): Promise<void> {
  if (checking.value) return;
  checking.value = true;
  beginJob('drc', board);
  try {
    const answer = await api.pcbDrc(board);
    // The check's answer has KiCad's findings, so it stands over any plain question asked while it
    // ran (the server answered that one before it had them): those in flight are dropped.
    findingsAsked++;
    findings.value = answer;
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
    void loadFindings(board);
  } finally {
    checking.value = false;
    endJob();
  }
}

function show(next: PcbScene) {
  if (scene.value?.board !== next.board) {
    selection.value = [];
    pickedKeepout.value = null;
    pickedRoute.value = null;
    pickedFinding.value = null;
    findings.value = null;
    history.value = NO_HISTORY;
  } else {
    const paths = new Set(next.parts.map((p) => p.path));
    selection.value = selection.value.filter((p) => paths.has(p));
    if (pickedKeepout.value && !next.keepouts.some((k) => k.name === pickedKeepout.value)) pickedKeepout.value = null;
    const r = pickedRoute.value;
    const record = r && next.routes.find((n) => n.net === r.net);
    if (r && (!record || (r.track != null && r.track > record.tracks.length) || (r.via != null && r.via > record.vias.length))) {
      pickedRoute.value = null;
    }
  }
  scene.value = next;
  // What is wrong with it may have changed with where things are.
  void loadFindings(next.board);
}

/** Send changes to the server. True if it wrote them; the scene is then the new one. */
async function send(changes: PcbChange[]): Promise<boolean> {
  const current = scene.value;
  if (!current) return false;
  try {
    const answer = await api.pcbEdit({ board: current.board, stamp: current.layoutStamp, changes });
    show(answer.scene);
    // The routed copper is gone: what the last route said no longer holds.
    const { [current.board]: _gone, ...rest } = pcbReports.value;
    pcbReports.value = rest;
    return true;
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
    if (e instanceof ApiFailure && e.status === 409) {
      // The file is not what this page had: what it could undo is not either.
      history.value = NO_HISTORY;
      try {
        show(await api.pcbScene(current.board));
      } catch {
        // The next load says why.
      }
    }
    return false;
  }
}

/** Make a change to the routes file (delete, lock, unlock), as `netsmith routes` makes it. The server
 * places the board again from the file, unrouted, and says what it did; a refusal says why and
 * changes nothing. Not a step of undo yet. True if it was written. */
export function changeRoutes(change: RoutesChange): Promise<boolean> {
  return enqueue(async () => {
    const current = scene.value;
    if (!current) return false;
    try {
      const answer = await api.pcbRoutes({ board: current.board, stamp: current.routesStamp, change });
      show(answer.scene);
      // The routed copper is gone: what the last route said no longer holds.
      const { [current.board]: _gone, ...rest } = pcbReports.value;
      pcbReports.value = rest;
      say(answer.said);
      return true;
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
      if (e instanceof ApiFailure && e.status === 409) {
        try {
          show(await api.pcbScene(current.board));
        } catch {
          // The next load says why.
        }
      }
      return false;
    }
  });
}

/** Make an edit: the changes are written, and it is one step of undo. */
export function commit(changes: PcbChange[], label: string): Promise<boolean> {
  if (changes.length === 0) return Promise.resolve(false);
  return enqueue(async () => {
    const ok = await send(changes);
    if (ok) {
      const step: Step = { label, changes };
      history.value = record(history.value, step);
    }
    return ok;
  });
}

export function undo(): Promise<void> {
  return enqueue(async () => {
    const u = undoing(history.value);
    if (!u) return;
    if (await send(u.run)) {
      history.value = u.next;
      say(`Undid: ${u.step.label}`);
    }
  });
}

export function redo(): Promise<void> {
  return enqueue(async () => {
    const r = redoing(history.value);
    if (!r) return;
    if (await send(r.run)) {
      history.value = r.next;
      say(`Redid: ${r.step.label}`);
    }
  });
}

/** What the board last wrote to the page's selection, so that following the page's selection back
 * onto the board does not undo it (a box of several parts is no single selection). */
export const wrote: { sel: Selection | null | undefined } = { sel: undefined };

/** The parts picked on the board by the person. One part is the page's selection; none clears it
 * unless it is a net; several leave the page with no single thing selected. */
export function select(paths: string[]) {
  selection.value = paths;
  pickedKeepout.value = null;
  pickedRoute.value = null;
  tellPage(paths);
}

/** Pick a record of the routes file, or one shape of it; the parts and the keepout picked are let go. */
export function pickRoute(pick: RoutePick | null) {
  pickedRoute.value = pick;
  if (pick) {
    pickedKeepout.value = null;
    selection.value = [];
    tellPage([]);
  }
}

function tellPage(paths: string[]) {
  const now = shared.peek();
  const next = paths.length === 1 ? partSel(paths[0]) : paths.length === 0 && now?.kind === 'net' ? now : null;
  if (!sameSelection(now, next)) {
    wrote.sel = next;
    setSelection(next);
  }
}

export function pickKeepout(name: string | null) {
  pickedKeepout.value = name;
  if (name) {
    pickedRoute.value = null;
    selection.value = [];
    tellPage([]);
  }
}
