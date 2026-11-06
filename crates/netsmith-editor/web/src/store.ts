// What the page knows, as signals, and the things it does with it.

import { computed, effect, signal } from '@preact/signals';

import type {
  Analysis,
  BoardDetail,
  BuildReport,
  CatalogRefresh,
  FabReport,
  Location,
  OrderReport,
  PcbReport,
  Problem,
  Project,
} from './api';
import { AnalysisPending, ApiFailure, Events, api } from './client';
import { nextDelay } from './feedback/early';
import {
  type Layout,
  type Section,
  openFrom,
  parseLayout,
  show,
  undoOpen,
  visiblePanes,
} from './layout';
import { booting, noteElapsed, notify, setSocket, track } from './feedback/state';
import { load, save as keep } from './storage';
import {
  type Selection,
  bindBoard,
  declarationOf,
  exists,
  netSel,
  partByRefdes,
  partSel,
  refdesOf,
  selection,
  setSelection,
} from './selection';
import { covers } from './util';
import { EMPTY, type History, forget, leaving, stepBack, stepForward, visit } from './source/navhistory';
import { reorder } from './source/tabs';

export { selection };

export type ViewKind = Section;

export interface OpenFile {
  path: string;
  text: string;
  /** The text as last read from or written to disk. */
  saved: string;
  stamp: string;
  readonly: boolean;
  /** Set when the file changed on disk while the page held edits. */
  conflict: boolean;
  /** Goes up when the text is replaced from outside the editor (read again from disk). */
  rev: number;
}

export const project = signal<Project | null>(null);
/** The check of what is on disk. */
export const analysis = signal<Analysis | null>(null);
/** The check of the text being edited, before it is saved. */
export const live = signal<Analysis | null>(null);
export const connected = signal(false);

export const openFiles = signal<Record<string, OpenFile>>({});
export const tabs = signal<string[]>([]);
export const activeFile = signal<string | null>(null);

const LAYOUT_KEY = 'netsmith.layout.v1';
const BOARD_KEY = 'netsmith.board.v1';

/** What the window shows: one section, or two side by side. Kept in localStorage. */
export const layout = signal<Layout>(parseLayout(load<unknown>(LAYOUT_KEY, null)));
effect(() => keep(LAYOUT_KEY, layout.value));
export const windowWidth = signal(typeof window === 'undefined' ? 2000 : window.innerWidth);
/** The panes on screen: a window too narrow for two shows one. */
export const visible = computed(() => visiblePanes(layout.value, windowWidth.value));
/** Where a click-through came from, so Back can put it back. */
export const back = signal<{ pane: number; section: Section } | null>(null);
/** Whether the notice about a second pane the editor opened a section in is showing. It is shown
 * once: dismissed, or the pane closed, it does not come back. */
export const paneNotice = signal(false);
const NOTICE_KEY = 'netsmith.paneNotice.v1';
export function dismissPaneNotice() {
  paneNotice.value = false;
  keep(NOTICE_KEY, true);
}
function noticeIf(other: boolean) {
  if (other && !load<boolean>(NOTICE_KEY, false)) paneNotice.value = true;
}
let pushed = false;

/** The user picks a section for a pane (a tab, a number key): it replaces what the pane shows,
 * or swaps with the other pane when that has it. */
export function showSection(section: Section, pane = layout.peek().focus) {
  back.value = null;
  layout.value = show(layout.peek(), section, pane);
}

/** A section is asked for by something other than a tab (the Build button, a panel in the tree):
 * left alone when it is on screen, else in the pane that was not clicked. */
export function revealSection(section: Section) {
  const r = openFrom(layout.peek(), windowWidth.peek(), section, layout.peek().focus);
  layout.value = r.layout;
  noticeIf(r.other);
}

/** A click-through to the source: it opens in the other pane of a split, else replaces what the
 * one pane shows, and Back returns. */
export function revealSource() {
  const r = openFrom(layout.peek(), windowWidth.peek(), 'source', layout.peek().focus);
  layout.value = r.layout;
  noticeIf(r.other);
  if (r.back) {
    back.value = r.back;
    history.pushState({ netsmithBack: true }, '');
    pushed = true;
  }
}

/** Put the pane that a click-through changed back as it was. */
export function goBack() {
  const b = back.peek();
  if (!b) return;
  back.value = null;
  layout.value = undoOpen(layout.peek(), b);
}

/** Back, by the button or a key: through the browser's history, so its Back button agrees. */
export function requestBack() {
  if (pushed) {
    pushed = false;
    history.back();
  } else goBack();
}

export function setFocusPane(index: number) {
  if (layout.peek().focus !== index) layout.value = { ...layout.peek(), focus: index };
}

export const board = signal<string | null>(null);
bindBoard(() => board.peek());
/** The panel whose preview the PCB view shows instead of the board (ADR 0040), if any. */
export const panelShown = signal<string | null>(null);
export const detail = signal<BoardDetail | null>(null);

/** Asks the editor to put the cursor at a place. `nonce` makes the same place twice a new ask. */
export const reveal = signal<{ at: Location; name?: string; nonce: number; quiet?: boolean } | null>(null);
export const cursor = signal<{ file: string; line: number; column: number } | null>(null);
/** The places the source's cursor was taken to, for Back and Forward (Alt+Left, Alt+Right). */
export const navHistory = signal<History>(EMPTY);
/** The designators of the parts the shared selection stands for (a part, the parts on a net, the
 * parts under a module), for the views that list designators. */
export const selectedRefdes = computed(() => refdesOf(detail.value, selection.value));
/** The last `fab` run of each board. */
export const fabReports = signal<Record<string, FabReport>>({});
/** A run asked for by the top bar, taken by the build view. */
export const run = signal<{ what: 'build' | 'fab'; nonce: number } | null>(null);
/** A long job the page asked for (`route`, `fab`, `build`): what the server has said of it and
 * when it started. The page stays usable while it runs. */
export interface Job {
  task: string;
  board: string;
  started: number;
  lines: string[];
}
export const job = signal<Job | null>(null);

export function beginJob(task: string, board: string) {
  job.value = { task, board, started: Date.now(), lines: [] };
}

export function endJob() {
  job.value = null;
}

/** What the last place-and-route of each board said, and a counter that goes up when the drawing
 * must be fetched again. Kept here, not in the view, so that a route started on the PCB tab
 * is not lost when another tab is opened while it runs. */
export const pcbReports = signal<Record<string, PcbReport>>({});
export const pcbStamp = signal(0);

/** Place and route a board. The report, or null when the server could not do it (a toast says why). */
export async function routeBoard(name: string): Promise<PcbReport | null> {
  beginJob('route', name);
  try {
    const report = await api.route(name);
    pcbReports.value = { ...pcbReports.value, [name]: report };
    pcbStamp.value++;
    return report;
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
    return null;
  } finally {
    endJob();
  }
}

/** What the last "Check parts against LCSC" did. Kept here so it is not lost when another tab is opened. */
export const catalogReport = signal<CatalogRefresh | null>(null);

export async function checkParts(dry: boolean, offline: boolean) {
  beginJob('catalog', '');
  try {
    catalogReport.value = await api.refreshCatalogs(dry, offline);
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
  } finally {
    endJob();
  }
}

/** The last run of each order of `netsmith.toml`, by the order's name. */
export const orderReports = signal<Record<string, OrderReport>>({});
/** The order picked in the build view. */
export const pickedOrder = signal<string | null>(null);

/** When each last run finished (`build`, `fab:<board>`, `order:<name>`), for "3 min ago". */
export const ranAt = signal<Record<string, number>>({});
export const markRan = (key: string) => (ranAt.value = { ...ranAt.value, [key]: Date.now() });

/** What the last schematic build said. */
export const buildReport = signal<BuildReport | null>(null);

export const dirty = (f: OpenFile) => f.text !== f.saved;

export const anyDirty = computed(() => Object.values(openFiles.value).some(dirty));

/** The problems on show: the unsaved text's if there is any, else the disk's. */
export const shown = computed<Analysis | null>(() =>
  anyDirty.value && live.value ? live.value : analysis.value,
);

export const problems = computed<Problem[]>(() => shown.value?.problems ?? []);

/** The parts whose declarations include the line the cursor is on. */
export const lineParts = computed<string[]>(() => {
  const c = cursor.value;
  const d = detail.value;
  if (!c || !d) return [];
  return d.parts
    .filter((p) => [p.decl, ...p.scopes].some((at) => at && covers(at, c.file, c.line)))
    .map((p) => p.refdes);
});

let events: Events | null = null;
let nonce = 0;

/** A short word to the person: a toast. A failure stays a little longer. `notify` (in
 * `feedback/state.ts`) says more, with a cause and buttons. */
export function say(text: string, bad = false) {
  notify({ title: text, kind: bad ? 'bad' : 'info' });
}

export async function start() {
  events = new Events(
    (message) => {
      switch (message.type) {
        case 'rebuilt':
          analysis.value = message.analysis;
          rebuilt++;
          noteElapsed(message.analysis.elapsedMs);
          track({ type: 'rebuilt' }, message.elapsedMs ?? message.analysis.elapsedMs);
          for (const wake of wakers.splice(0)) wake();
          void onRebuilt();
          break;
        case 'progress': {
          if (message.task === 'analyse') {
            // The analysis of the files on disk: the stage and its count, not a line of text.
            if (message.stage !== undefined) {
              track({ type: 'progress', now: Date.now(), stage: message.stage, board: message.board, done: message.done ?? 0, total: message.total ?? 0 });
            }
            break;
          }
          const j = job.value;
          if (j && j.task === message.task) {
            job.value = { ...j, lines: [...j.lines.slice(-39), message.message] };
          }
          break;
        }
        case 'checked': {
          const latest = !!events?.isLatest(message.id);
          track({ type: 'checked', id: message.id, latest });
          if (latest) live.value = message.analysis;
          break;
        }
        case 'analysing':
          // The server began to read the project, for whatever reason (a file changed outside the
          // editor, or it is the first time): its own start time is the clock.
          track({ type: 'analysing', since: message.since });
          break;
        default:
          // A message type added to `ServerMessage` without a case above fails the build here
          // (`message` is no longer `never`). A newer server's unknown message is ignored at run time.
          message satisfies never;
      }
    },
    (open) => {
      connected.value = open;
      setSocket(open);
      if (open) void refresh();
    },
  );
  await refresh();
}

/** How many times the socket has brought an analysis, and who waits for the next one. */
let rebuilt = 0;
const wakers: (() => void)[] = [];

/** Wait for the socket's `rebuilt`, or `ms` if it does not come. */
function untilRebuilt(ms: number): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(done, ms);
    function done() {
      clearTimeout(timer);
      const at = wakers.indexOf(done);
      if (at >= 0) wakers.splice(at, 1);
      resolve();
    }
    wakers.push(done);
  });
}

/** The analysis of the files on disk. The first request asks to be told while the server's first
 * analysis runs (it answers at once; a request that waits would leave the page with nothing for the
 * 30 s a big project takes): the page shows its skeleton and the time, and takes the analysis from
 * the socket's `rebuilt`, asking again with growing pauses in case that push is lost. `null` when
 * the socket delivered it (the page is already up to date). */
async function readAnalysis(): Promise<Analysis | null> {
  const before = rebuilt;
  let delay = 0;
  for (;;) {
    try {
      const a = await api.analysis(true);
      booting.value = null;
      return a;
    } catch (e) {
      if (!(e instanceof AnalysisPending)) throw e;
      booting.value = { at: Date.now(), elapsedMs: e.elapsedMs };
      delay = nextDelay(delay, connected.peek());
      await untilRebuilt(delay);
      if (rebuilt !== before) return null;
    }
  }
}

let refreshing: Promise<void> | null = null;
let refreshAgain = false;
let readOnce = false;

/** Read the project and the analysis; a second call while one runs makes the first read again when
 * it ends, so a socket that reconnects during the first read is not lost. */
function refresh(): Promise<void> {
  if (refreshing) {
    // The first read is what a socket that opens meanwhile would start: it needs no second one.
    if (readOnce) refreshAgain = true;
    return refreshing;
  }
  refreshing = (async () => {
    try {
      do {
        refreshAgain = false;
        await refreshOnce();
        readOnce = true;
      } while (refreshAgain);
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}

async function refreshOnce() {
  // The tree and the source need no analysis: they are on screen while it runs.
  const p = await api.project();
  project.value = p;
  if (tabs.value.length === 0) {
    const first = p.files.find((f) => f.role === 'top_level');
    if (first) await openFile(first.path);
  }
  const a = await readAnalysis();
  if (a) {
    analysis.value = a;
    noteElapsed(a.elapsedMs);
    await onRebuilt(false);
  }
}

/** After a file or folder was made, moved or removed: the tree and the check are read again. */
export async function refreshAfterChange() {
  const [p, a] = await Promise.all([api.project(), api.analysis()]);
  project.value = p;
  analysis.value = a;
  await onRebuilt(false);
}

const under = (path: string, from: string, folder: boolean) => path === from || (folder && path.startsWith(`${from}/`));

/** A file or folder was moved: the open files in it keep their text and edits at the new path. */
export function renameOpen(from: string, to: string, folder: boolean) {
  const moved = (path: string) => (path === from ? to : `${to}${path.slice(from.length)}`);
  const files: Record<string, OpenFile> = {};
  for (const [path, f] of Object.entries(openFiles.value)) {
    files[under(path, from, folder) ? moved(path) : path] = under(path, from, folder) ? { ...f, path: moved(path) } : f;
  }
  openFiles.value = files;
  tabs.value = tabs.value.map((t) => (under(t, from, folder) ? moved(t) : t));
  const active = activeFile.value;
  if (active && under(active, from, folder)) activeFile.value = moved(active);
}

/** A file or folder was removed: its tabs go, with edits that were not saved. */
export function dropOpen(path: string, folder: boolean) {
  const gone = (p: string) => under(p, path, folder);
  const rest = tabs.value.filter((t) => !gone(t));
  tabs.value = rest;
  navHistory.value = forget(navHistory.value, gone);
  openFiles.value = Object.fromEntries(Object.entries(openFiles.value).filter(([p]) => !gone(p)));
  if (activeFile.value && gone(activeFile.value)) activeFile.value = rest[rest.length - 1] ?? null;
  if (!anyDirty.peek()) live.value = null;
}

/** The files were built again: the tree, the board and the open files may have changed. */
async function onRebuilt(tree = true) {
  const a = analysis.value;
  if (!a) return;
  if (tree) project.value = await api.project();
  const names = a.boards.map((b) => b.name);
  if (!board.value || !names.includes(board.value)) {
    // The board the person had last, if it is still there.
    const kept = load<string | null>(BOARD_KEY, null);
    board.value = kept && names.includes(kept) ? kept : (names[0] ?? null);
  }
  live.value = null;
  pcbReports.value = {};
  void loadDetail();
  // A file that changed on disk and has no edits here is read again.
  for (const f of Object.values(openFiles.value)) {
    if (f.readonly) continue;
    try {
      const fresh = await api.file(f.path);
      if (fresh.stamp === f.stamp) continue;
      if (dirty(f)) {
        setFile({ ...f, conflict: true });
      } else {
        setFile({ ...f, text: fresh.text, saved: fresh.text, stamp: fresh.stamp, conflict: false, rev: f.rev + 1 });
      }
    } catch {
      // A file that is gone stays as it is, and a save says why it fails.
    }
  }
}

export async function loadDetail() {
  const name = board.value;
  if (!name) {
    detail.value = null;
    return;
  }
  try {
    const d = await api.board(name);
    if (board.value === name) detail.value = d;
  } catch {
    detail.value = null;
  }
}

effect(() => {
  // A new board needs its own detail.
  if (board.value !== detail.peek()?.name) void loadDetail();
});

function setFile(f: OpenFile) {
  openFiles.value = { ...openFiles.value, [f.path]: f };
}

export function edit(path: string, text: string) {
  const f = openFiles.value[path];
  if (!f || f.text === text) return;
  setFile({ ...f, text });
  if (text === f.saved) {
    live.value = null;
    return;
  }
  scheduleCheck(path);
}

let timer: ReturnType<typeof setTimeout> | undefined;

function scheduleCheck(path: string) {
  clearTimeout(timer);
  timer = setTimeout(() => {
    const f = openFiles.value[path];
    if (!f || f.readonly) return;
    const id = events?.check(path, f.text);
    if (id != null) track({ type: 'check-sent', id, now: Date.now() });
  }, 350);
}

export async function openFile(path: string): Promise<void> {
  if (!openFiles.value[path]) {
    const content = await api.file(path);
    setFile({
      path,
      text: content.text,
      saved: content.text,
      stamp: content.stamp,
      readonly: content.readonly,
      conflict: false,
      rev: 0,
    });
  }
  if (!tabs.value.includes(path)) tabs.value = [...tabs.value, path];
  activeFile.value = path;
}

export function closeFile(path: string) {
  const f = openFiles.value[path];
  if (f && dirty(f) && !confirm(`${path} has changes that are not saved. Close it anyway?`)) return;
  const rest = tabs.value.filter((t) => t !== path);
  tabs.value = rest;
  const { [path]: _gone, ...others } = openFiles.value;
  openFiles.value = others;
  if (activeFile.value === path) activeFile.value = rest[rest.length - 1] ?? null;
  if (!anyDirty.peek()) live.value = null;
}

const saving = new Set<string>();

export async function save(path: string) {
  const f = openFiles.value[path];
  if (!f || f.readonly || !dirty(f) || saving.has(path)) return;
  saving.add(path);
  try {
    const saved = await api.save(path, f.text, f.stamp);
    // Edits made while the write was on its way stay edits.
    const now = openFiles.value[path] ?? f;
    setFile({ ...now, saved: f.text, stamp: saved.stamp, conflict: false });
    notify({ kind: 'ok', title: `Saved ${path}`, ms: 2200 });
    void awaitRebuild();
  } catch (e) {
    const status = e instanceof ApiFailure ? e.status : 0;
    const now = openFiles.value[path];
    if (status === 409 && now) setFile({ ...now, conflict: true });
    say(e instanceof Error ? e.message : String(e), true);
  } finally {
    saving.delete(path);
  }
}

/** After a file was written the server builds the project again: the page is analysing until that
 * is done. Asking for the analysis waits for the build, so this ends when the answer is here,
 * even if no file the server watches changed, or the push is lost. */
async function awaitRebuild() {
  track({ type: 'saved', now: Date.now() });
  try {
    const a = await api.analysis();
    if (a.version > (analysis.peek()?.version ?? 0)) {
      analysis.value = a;
      void onRebuilt();
    }
  } catch {
    // The socket's push, or the next refresh, brings it.
  } finally {
    track({ type: 'rebuilt' });
  }
}

/** Discard the page's edits to a file that changed on disk and read the disk's. */
export async function reload(path: string) {
  const fresh = await api.file(path);
  setFile({
    path,
    text: fresh.text,
    saved: fresh.text,
    stamp: fresh.stamp,
    readonly: fresh.readonly,
    conflict: false,
    rev: (openFiles.value[path]?.rev ?? 0) + 1,
  });
  live.value = null;
}

/** Open the file at a place and put the cursor there. `name` is the definition whose line it
 * is, when that is all that is known (the tree): the cursor goes to the name on the line.
 * With `quiet` the source is not brought to the front: its cursor is moved where it is, so it is
 * at the place when the person looks at it. */
export async function goTo(at: Location, name?: string, quiet = false, record = !quiet) {
  try {
    await openFile(at.file);
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
    return;
  }
  if (record) {
    // Where the cursor was, and where it goes: Back returns to the first.
    const c = cursor.peek();
    navHistory.value = visit(leaving(navHistory.peek(), c && openFiles.peek()[c.file] ? { file: c.file, line: c.line, column: c.column } : null), { file: at.file, line: at.line, column: at.column });
  }
  if (!quiet) revealSource();
  reveal.value = { at, name, nonce: ++nonce, quiet };
}

const placeAt = (p: { file: string; line: number; column: number }): Location => ({ file: p.file, line: p.line, column: p.column, endLine: p.line, endColumn: p.column });

/** Back through the places the cursor was taken to. False when there is nowhere to go. */
export async function navBack(): Promise<boolean> {
  const step = stepBack(navHistory.peek());
  if (!step) return false;
  navHistory.value = step.history;
  await goTo(placeAt(step.place), undefined, false, false);
  return true;
}

export async function navForward(): Promise<boolean> {
  const step = stepForward(navHistory.peek());
  if (!step) return false;
  navHistory.value = step.history;
  await goTo(placeAt(step.place), undefined, false, false);
  return true;
}

/** A tab dragged to a new place: before `before`, or to the end. */
export function moveTab(path: string, before: string | null) {
  tabs.value = reorder(tabs.value, path, before);
}

/** A part picked by its designator (a list that only knows the letters): it is selected in every
 * view, and with `jump` the source opens at its declaration. */
export function pickPart(refdes: string | null, jump = false) {
  if (!refdes) {
    setSelection(null);
    return;
  }
  const part = partByRefdes(detail.value, refdes);
  if (!part) return;
  setSelection(partSel(part.path));
  if (jump && part.decl) void goTo(part.decl);
}

/** A net picked (or none): lit in every view. A net has no line in the source. */
export function pickNet(name: string | null) {
  setSelection(name ? netSel(name) : null);
}

/** Open the source at the declaration of a selection; a net has none, and that is said. */
export function showInSource(sel: Selection, quiet = false) {
  const at = declarationOf(detail.value, sel);
  if (at) void goTo(at, undefined, quiet);
  else if (!quiet) say(sel.kind === 'net' ? 'A net has no line in the source: only the board names it.' : 'No declaration is known for it.');
}

/** Ask for a run of the build view (`Write KiCad files`, `Make fab files`). */
export function requestRun(what: 'build' | 'fab') {
  revealSection('build');
  run.value = { what, nonce: Date.now() };
}

// A selection the board no longer has (the source changed) is dropped.
effect(() => {
  const d = detail.value;
  const s = selection.value;
  if (d && s && d.name === board.value && !exists(d, s)) setSelection(null);
});

export function chooseBoard(name: string) {
  board.value = name;
  keep(BOARD_KEY, name);
  setSelection(null);
  // Another board is not the panel's.
  const shown = analysis.value?.panels.find((p) => p.name === panelShown.value);
  if (shown && analysis.value?.boards.find((x) => x.name === name)?.definition !== shown.board) {
    panelShown.value = null;
  }
}

export function viewBoardOfDefinition(kind: string, name: string) {
  const a = analysis.value;
  if (!a) return;
  if (kind === 'board') {
    const b = a.boards.find((x) => x.definition === name);
    if (b) chooseBoard(b.name);
  } else if (kind === 'system') {
    const s = a.systems.find((x) => x.name === name);
    if (s?.boards[0]) chooseBoard(s.boards[0]);
  } else if (kind === 'panel') {
    // The panel is drawn in the PCB view, over the board it repeats.
    const p = a.panels.find((x) => x.name === name);
    const b = p && a.boards.find((x) => x.definition === p.board && !x.system);
    if (p && b) {
      chooseBoard(b.name);
      panelShown.value = p.name;
      // The click was in the source's tree: with a second pane the panel opens there; with one
      // pane the source stays (the click also went to the panel's code) and the PCB section shows it.
      if (visible.peek().length === 2) revealSection('pcb');
    }
  }
}

/** The name `system`/`board` views are asked for: the system of the board if it is in one. */
export const systemName = computed(() => {
  const a = analysis.value;
  const b = board.value;
  return a?.boards.find((x) => x.name === b)?.system ?? null;
});
