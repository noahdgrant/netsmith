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
import { ApiFailure, Events, api } from './client';
import { covers } from './util';

export type ViewKind = 'schematic' | 'power' | 'system' | 'bom' | 'pcb' | 'build';

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

export const activeView = signal<ViewKind>('schematic');
export const board = signal<string | null>(null);
/** The panel whose preview the PCB view shows instead of the board (ADR 0040), if any. */
export const panelShown = signal<string | null>(null);
export const detail = signal<BoardDetail | null>(null);

/** Asks the editor to put the cursor at a place. `nonce` makes the same place twice a new ask. */
export const reveal = signal<{ at: Location; nonce: number } | null>(null);
export const cursor = signal<{ file: string; line: number; column: number } | null>(null);
/** The part picked in a view. */
export const selected = signal<string | null>(null);
/** The last `fab` run of each board. */
export const fabReports = signal<Record<string, FabReport>>({});
/** A run asked for by the top bar, taken by the build view. */
export const run = signal<{ what: 'build' | 'fab'; nonce: number } | null>(null);
export const showEditor = signal(true);
export const showView = signal(true);
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

export async function routeBoard(name: string) {
  beginJob('route', name);
  try {
    const report = await api.route(name);
    pcbReports.value = { ...pcbReports.value, [name]: report };
    pcbStamp.value++;
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
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

/** What the last schematic build said. */
export const buildReport = signal<BuildReport | null>(null);

export const problemsOpen = signal(false);
/** Whether the notes section of the problems panel is open. */
export const notesOpen = signal(false);
export const toast = signal<{ text: string; bad: boolean } | null>(null);

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

export function say(text: string, bad = false) {
  toast.value = { text, bad };
  setTimeout(() => {
    if (toast.value?.text === text) toast.value = null;
  }, 4000);
}

export async function start() {
  events = new Events(
    (message) => {
      if (message.type === 'rebuilt') {
        analysis.value = message.analysis;
        void onRebuilt();
      } else if (message.type === 'progress') {
        const j = job.value;
        if (j && j.task === message.task) {
          job.value = { ...j, lines: [...j.lines.slice(-39), message.message] };
        }
      } else if (events?.isLatest(message.id)) {
        live.value = message.analysis;
      }
    },
    (open) => {
      connected.value = open;
      if (open) void refresh();
    },
  );
  await refresh();
}

async function refresh() {
  const [p, a] = await Promise.all([api.project(), api.analysis()]);
  project.value = p;
  analysis.value = a;
  await onRebuilt(false);
  if (tabs.value.length === 0) {
    const first = p.files.find((f) => f.role === 'top_level');
    if (first) await openFile(first.path);
  }
}

/** The files were built again: the tree, the board and the open files may have changed. */
async function onRebuilt(tree = true) {
  const a = analysis.value;
  if (!a) return;
  if (tree) project.value = await api.project();
  const names = a.boards.map((b) => b.name);
  if (!board.value || !names.includes(board.value)) board.value = names[0] ?? null;
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
    if (f && !f.readonly) events?.check(path, f.text);
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
  showEditor.value = true;
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
    say(`Saved ${path}`);
  } catch (e) {
    const status = e instanceof ApiFailure ? e.status : 0;
    const now = openFiles.value[path];
    if (status === 409 && now) setFile({ ...now, conflict: true });
    say(e instanceof Error ? e.message : String(e), true);
  } finally {
    saving.delete(path);
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

/** Open the file at a place and put the cursor there. */
export async function goTo(at: Location) {
  try {
    await openFile(at.file);
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
    return;
  }
  reveal.value = { at, nonce: ++nonce };
}

export function pickPart(refdes: string | null, jump = false) {
  selected.value = refdes;
  if (!jump || !refdes) return;
  const part = detail.value?.parts.find((p) => p.refdes === refdes);
  if (part?.decl) void goTo(part.decl);
}

export function chooseBoard(name: string) {
  board.value = name;
  selected.value = null;
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
      activeView.value = 'pcb';
    }
  }
}

/** The name `system`/`board` views are asked for: the system of the board if it is in one. */
export const systemName = computed(() => {
  const a = analysis.value;
  const b = board.value;
  return a?.boards.find((x) => x.name === b)?.system ?? null;
});
