// What the PCB canvas knows: the scene of the board, the selection, the tool, and the history
// of edits. An edit goes to the server, which writes it to the layout file and answers with
// the board placed again; the page never keeps a second copy of the layout.

import { effect, signal } from '@preact/signals';

import type {
  PcbChange,
  PcbCrossed,
  PcbFindings,
  PcbFill,
  PcbStitching,
  PcbNets,
  PcbPours,
  PcbRegenerated,
  PcbRoutesEdited,
  PcbScene,
  PcbShove,
  PcbShoved,
  PcbWalk,
  RoutesChange,
} from './api';
import { ApiFailure, api, wasCancelled } from './client';
import { livePlace, placingSaid } from './pcb/liveState';
import { type History, NO_HISTORY, type Run, type Step, record, redoing, undoing } from './pcb/edits';
import { proposalSaid } from './pcb/regenerate';
import { labelOf } from './pcb/routes';
import type { Side } from './pcb/side';
import {
  type Answer,
  type Rerouted,
  type ShoveAnswer,
  answerTo,
  latestOnly,
  saidOf,
  shoveAnswerTo,
  shoveKey,
  walkKey,
} from './pcb/walk';
import { type Selection, partPathsOf, partSel, sameSelection, selection as shared, setSelection } from './selection';
import { beginJob, board, detail, endJob, job, pcbReports, say } from './store';
import { litModules, litOn, setLit } from './pcb/modules';

export const scene = signal<PcbScene | null>(null);
/** The fill of the board's pours, planes and zones, as the server filled the placement it names (drawn only while that is the scene's). */
export const pours = signal<PcbPours | null>(null);

/** How the fill joins the pads, its removed islands and its starved pads (`GET /api/pcb/fill`, issue 1188), as the server filled
 * the placement it names: the 2D view marks the islands and the pads (`pcb/fillMarks.ts`). */
export const fillReport = signal<PcbFill | null>(null);
/** The stitching vias the export will have that the routes file does not (`GET /api/pcb/stitching`, issue 1489), of the placement
 * it names: the 2D view draws them in a tone of their own (`pcb/stitchMarks.ts`). */
export const stitching = signal<PcbStitching | null>(null);
/** The table of the board's nets (`GET /api/pcb/nets`, issue 1495), of the placement it names: the 2D view marks the copper a
 * net's length does not count, the dead stubs and second ways (`pcb/uncountedMarks.ts`). */
export const netsReport = signal<PcbNets | null>(null);
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
/** Select (and drag), draw a keepout, draw a track, or place a via. */
export type Tool = 'select' | 'keepout' | 'track' | 'via';
export const tool = signal<Tool>('select');
/** The side of the board the 2D view looks at (the key B goes through them): both faces as the file has them, until a person turns the board over. */
export const pcbSide = signal<Side>('both');
/** The layer the track tool draws on, by its word: `top`, `bottom` or an inner signal layer of the process (`inner2`), one of the
 * scene's `trackLayers`. */
export const trackLayer = signal<string>('top');

/** The via the via tool places (issue 1071): `''` for the through via, else a blind or a buried via's two layers, `top,inner1`, one
 * of the scene's `viaOffers`. */
export const viaSpan = signal<string>('');

/** How the track tool treats the unlocked copper of the routes file that is in the way (issue 726): shoved aside (the default), or
 * only walked round, which leaves every shape where it is. */
export const trackShove = signal<boolean>(true);
export const history = signal<History>(NO_HISTORY);
/** What is wrong with the board as it is now, and the finding picked in the list. */
export const findings = signal<PcbFindings | null>(null);
export const pickedFinding = signal<string | null>(null);
/** Whether KiCad's check is running. */
export const checking = signal(false);

/** A regeneration of one module the server proposed (issue 825), drawn over the board until it is
 * accepted (its changes sent as one edit) or dropped; with the layout stamp it was made on. */
export const proposal = signal<{ board: string; stamp: string; answer: PcbRegenerated } | null>(null);

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

/** The board whose placing a person cancelled (issue 764): the view says so and offers to place it again. */
export const placingCancelled = signal<string | null>(null);

/** Take the scene of a board from the server. The selection keeps what is still there. While the server places the board
 * the socket shows the parts where the placer has them (`liveState.ts`); a placing that is cancelled leaves no scene. */
export function loadScene(board: string): Promise<void> {
  return enqueue(async () => {
    if (scene.peek()?.board !== board) {
      // A placing of this board begins: what was shown of another placing is not of it.
      livePlace.value = null;
      placingSaid.value = null;
    }
    placingCancelled.value = null;
    try {
      const next = await api.pcbScene(board);
      show(next);
      sceneError.value = null;
    } catch (e) {
      scene.value = null;
      if (wasCancelled(e)) {
        placingCancelled.value = board;
        sceneError.value = null;
      } else sceneError.value = e instanceof Error ? e.message : String(e);
    }
  });
}

/** Stop the placing of the board that runs: its scene answers `cancelled` and the view offers to place it again. */
export async function cancelPlacing(): Promise<void> {
  try {
    await api.cancelPlacing();
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
  }
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
  // A proposal made on another placement no longer says what Accept would do.
  const p = proposal.peek();
  if (p && (p.board !== next.board || p.stamp !== next.layoutStamp)) proposal.value = null;
  // What is wrong with it may have changed with where things are.
  void loadFindings(next.board);
  // So may the pours and zones, which come after the board is drawn.
  void loadPours(next.board)
    .then(() => loadFill(next.board))
    .then(() => loadStitching(next.board))
    .then(() => loadNets(next.board));
}

let poursAsked = 0;

/** Ask the server for the fill of the board as it is placed now; the answer of the last ask is kept, and
 * the fill of another board goes at once. A board that cannot be filled is drawn with no fill. */
export async function loadPours(board: string): Promise<void> {
  const mine = ++poursAsked;
  if (pours.peek()?.board !== board) pours.value = null;
  try {
    const answer = await api.pcbPours(board);
    if (mine === poursAsked) pours.value = answer.error ? null : answer;
  } catch {
    // The scene's own load says why the board cannot be placed; the board is drawn with no fill.
    if (mine === poursAsked) pours.value = null;
  }
}

let fillAsked = 0;

/** Ask the server for the report of the fill of the board as it is placed now, after its pours (the two fills would race for
 * the server's time); the answer of the last ask is kept. A board that cannot be filled has no marks. */
export async function loadFill(board: string): Promise<void> {
  const mine = ++fillAsked;
  if (fillReport.peek()?.board !== board) fillReport.value = null;
  try {
    const answer = await api.pcbFill(board);
    if (mine === fillAsked) fillReport.value = answer;
  } catch {
    if (mine === fillAsked) fillReport.value = null;
  }
}

let stitchingAsked = 0;

/** Ask the server for the stitching vias of the board as it is placed now, after its fill (one at a time on the server's time); the
 * answer of the last ask is kept. A board that cannot be placed has none. */
export async function loadStitching(board: string): Promise<void> {
  const mine = ++stitchingAsked;
  if (stitching.peek()?.board !== board) stitching.value = null;
  try {
    const answer = await api.pcbStitching(board);
    if (mine === stitchingAsked) stitching.value = answer;
  } catch {
    if (mine === stitchingAsked) stitching.value = null;
  }
}

let netsAsked = 0;

/** Ask the server for the table of the nets of the board as it is placed now, after its stitching; the answer of the last ask is
 * kept. A board that cannot be measured has none. */
export async function loadNets(board: string): Promise<void> {
  const mine = ++netsAsked;
  if (netsReport.peek()?.board !== board) netsReport.value = null;
  try {
    const answer = await api.pcbNets(board);
    if (mine === netsAsked) netsReport.value = answer;
  } catch {
    if (mine === netsAsked) netsReport.value = null;
  }
}

/** The server's last answer to where the track being drawn goes next (issue 726), with the ask it answers (`Answer.key`). */
export const walked = signal<Answer | null>(null);

/** The router's copper the track being drawn crosses, as the canvas shows it (null: none, or no track): the toolbar says so too. */
export const crossedShown = signal<PcbCrossed | null>(null);

/** The shapes the track being drawn shoves out of its way, and why a shove was not used where one was asked for (null: no track):
 * the toolbar's note says so. */
export const shoveShown = signal<{ shoved: PcbShoved; stuck: string | null } | null>(null);

/** Ask where a track would go: the answer lands in `walked`. Asks come one at a time, the newest first (`latestOnly`), so a
 * pointer that outruns the server asks for where it is. The page is busy while a request is out (`api` holds it), so a recipe
 * that waits for the page to be idle sees the answer. */
export const askWalk = latestOnly(
  (w: PcbWalk) => api.pcbWalk(w),
  (w, got) => {
    walked.value = answerTo(w, got);
  },
);

/** Forget the answer: no track is being drawn. */
export function clearWalk() {
  if (walked.value) walked.value = null;
}

/** The answer for `w`: the one the page has, or a request made now. */
export async function walkNow(w: PcbWalk): Promise<Answer> {
  const have = walked.value;
  if (have && have.key === walkKey(w)) return have;
  try {
    return answerTo(w, await api.pcbWalk(w));
  } catch (e) {
    return answerTo(w, e instanceof Error ? e : new Error(String(e)));
  }
}

/** The server's last answer to a via placed or a drag (issue 1009): whether it has room, and what it shoves aside. */
export const shoveAsked = signal<ShoveAnswer | null>(null);

/** Ask whether a via or the shapes of a drag have room where the pointer is, and what they shove aside: the answer lands in
 * `shoveAsked`, one ask at a time, the newest first, as `askWalk`. */
export const askShove = latestOnly(
  (s: PcbShove) => api.pcbShove(s),
  (s, got) => {
    shoveAsked.value = shoveAnswerTo(s, got);
  },
);

/** Forget the answer: no via or drag is under way. */
export function clearShove() {
  if (shoveAsked.value) shoveAsked.value = null;
}

/** The answer for `s`: the one the page has, or a request made now. */
export async function shoveNow(s: PcbShove): Promise<ShoveAnswer> {
  const have = shoveAsked.value;
  if (have && have.key === shoveKey(s)) return have;
  try {
    return shoveAnswerTo(s, await api.pcbShove(s));
  } catch (e) {
    return shoveAnswerTo(s, e instanceof Error ? e : new Error(String(e)));
  }
}

/** The router's crossed nets routed again round the track being drawn (issue 1009), with the ask it answers; asked after a walk that
 * crosses router copper, one at a time, the newest first. */
export const rerouted = signal<Rerouted | null>(null);
export const askReroute = latestOnly(
  (w: PcbWalk) => api.pcbReroute(w),
  (w, got) => {
    rerouted.value = { key: walkKey(w), answer: got instanceof Error ? null : got };
  },
);

/** Forget the re-route: nothing is crossed, or no track is drawn. */
export function clearReroute() {
  if (rerouted.value) rerouted.value = null;
}

/** Send changes to the server. True if it wrote them; the scene is then the new one. */
async function send(changes: PcbChange[]): Promise<boolean> {
  const current = scene.value;
  if (!current) return false;
  try {
    const answer = await api.pcbEdit({ board: current.board, stamp: current.layoutStamp, changes });
    show(answer.scene);
    if (answer.report) {
      // Back to a placement that was routed (an undo): its copper and what its route said.
      pcbReports.value = { ...pcbReports.value, [current.board]: answer.report };
    } else {
      // The routed copper is gone: what the last route said no longer holds.
      const { [current.board]: _gone, ...rest } = pcbReports.value;
      pcbReports.value = rest;
    }
    return true;
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
    if (e instanceof ApiFailure && e.status === 409) {
      // The layout file or the physical file is not what this page had: what it could undo is not either.
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

/** Send one edit of the routes file. Its answer, or null when it was refused (the toast says why). */
async function sendRoutes(changes: RoutesChange[]): Promise<PcbRoutesEdited | null> {
  const current = scene.value;
  if (!current) return null;
  try {
    const answer = await api.pcbRoutes({ board: current.board, stamp: current.routesStamp, changes });
    show(answer.scene);
    // The routed copper is gone: what the last route said no longer holds.
    const { [current.board]: _gone, ...rest } = pcbReports.value;
    pcbReports.value = rest;
    return answer;
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
    if (e instanceof ApiFailure && e.status === 409) {
      // The routes file changed on disk. Its steps of undo stay: each puts a record back only if
      // the file still has it as the step left it, and is refused with the reason if not.
      try {
        show(await api.pcbScene(current.board));
      } catch {
        // The next load says why.
      }
    }
    return null;
  }
}

/** Make one edit of the routes file (its changes are of one net: a track drawn, a via placed, a drag,
 * a delete, a lock), as `netsmith routes` makes it. The server places the board again from the file,
 * unrouted, and says what it did; a refusal says why and changes nothing. It is one step of undo,
 * which puts the net's record back as it was. True if it was written. */
export function changeRoutes(changes: RoutesChange[]): Promise<boolean> {
  if (changes.length === 0) return Promise.resolve(false);
  return enqueue(async () => {
    const wasRouted = scene.value?.routed ?? false;
    const answer = await sendRoutes(changes);
    if (!answer) return false;
    say(saidOf(answer.said, wasRouted));
    // One step for the whole edit, whatever number of nets it changed (a shove changes several).
    const changed = answer.undo.filter((u) => u.before !== u.after);
    if (changed.length > 0) {
      history.value = record(history.value, { label: labelOf(changes), routes: changed });
    }
    return true;
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

/** Ask the server to place `module` again with every other part held, with `seed`: the proposal is
 * drawn over the board and nothing is written. True if there is one; a refusal says why. */
export function propose(module: string, seed: number): Promise<boolean> {
  return enqueue(async () => {
    const current = scene.value;
    if (!current) return false;
    // While it is asked for, the server's steps show as a job (the progress of `regenerate`), unless another job
    // (a route) is running: that one keeps the place.
    const mine = !job.value;
    if (mine) beginJob('regenerate', current.board);
    try {
      const answer = await api.pcbRegenerate({ board: current.board, stamp: current.layoutStamp, module, seed });
      proposal.value = { board: current.board, stamp: current.layoutStamp, answer };
      say(proposalSaid(answer));
      return true;
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
      return false;
    } finally {
      if (mine && job.value?.task === 'regenerate') endJob();
    }
  });
}

/** The same module again with the next seed: another arrangement. */
export function proposeAnother(): Promise<boolean> {
  const p = proposal.value;
  return p ? propose(p.answer.module, p.answer.seed + 1) : Promise.resolve(false);
}

/** Keep the proposal: its changes are written as one edit, one step of undo. */
export function acceptProposal(): Promise<boolean> {
  const p = proposal.value;
  if (!p) return Promise.resolve(false);
  proposal.value = null;
  if (p.answer.changes.length === 0) {
    say(`Regenerate ${p.answer.module}: the parts are where they were`);
    return Promise.resolve(false);
  }
  return commit(p.answer.changes, `regenerate ${p.answer.module} (seed ${p.answer.seed})`);
}

/** Drop the proposal: nothing was written. */
export function dropProposal() {
  proposal.value = null;
}

/** Undo or redo a step: changes to the layout file, or a record of the routes file put back. */
async function runStep(run: Run): Promise<boolean> {
  return 'layout' in run ? send(run.layout) : (await sendRoutes(run.routes)) !== null;
}

export function undo(): Promise<void> {
  return enqueue(async () => {
    const u = undoing(history.value);
    if (!u) return;
    if (await runStep(u.run)) {
      history.value = u.next;
      say(`Undid: ${u.step.label}`);
    }
  });
}

export function redo(): Promise<void> {
  return enqueue(async () => {
    const r = redoing(history.value);
    if (!r) return;
    if (await runStep(r.run)) {
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

// A module picked in any view (a card of the schematic's overview, go to anything, `/api/view`) is lit on the board, as a
// click in the Modules tab lights it (issue 1129), whether or not the PCB view is on screen; one lit already keeps its
// colour. A module the board has no part in (the detail of the board says so) is not.
effect(() => {
  const sel = shared.value;
  if (sel?.kind !== 'module') return;
  const b = board.peek();
  const d = detail.peek();
  if (!b || (d && d.name === b && !partPathsOf(d, sel).length)) return;
  if (!litOn(b, litModules.peek()).some((m) => m.path === sel.key)) setLit(b, [{ path: sel.key, slot: 0 }]);
});
