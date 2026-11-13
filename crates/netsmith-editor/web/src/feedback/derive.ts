// What the dock and the status bar list, read from the store: the compiler's problems, the PCB's
// findings as one list of rows (the power model's problems are the compiler's: the analysis carries them); and what a click or F8 on a row does.

import { computed, effect, signal } from '@preact/signals';

import type { Location } from '../api';
import { askFocus3d } from '../pcb3d/inspectMode';
import { findings, loadFindings, pickedFinding, scene, select, wrote } from '../pcbstore';
import { type Selection, netSel, partSel, selectAndReveal, setSelection } from '../selection';
import {
  activeFile,
  analysis,
  anyDirty,
  board,
  cursor,
  detail,
  goTo,
  layout,
  live,
  revealSection,
  shown,
  visible,
} from '../store';
import { covers } from '../util';
import { activeRow, dock, filter, group, notify, setDockOpen } from './state';
import {
  type Row,
  type ViewScope,
  countRows,
  filterRows,
  locationOf,
  nextRow,
  rowOfFinding,
  rowOfProblem,
  sortRows,
  steps,
} from './problems';
import { type Banner, bannerFor } from './status';

/** The compiler's problems of what is on screen: the unsaved text's if there is any, else the disk's. */
const projectRows = computed<Row[]>(() => (shown.value?.problems ?? []).map(rowOfProblem));

const boardStale = computed(() => !!analysis.value?.boards.find((b) => b.name === board.value)?.stale);

/** The PCB's findings, as far as the page has asked for them (the PCB view does, and the dock). */
const pcbRows = computed<Row[]>(() => {
  const f = findings.value;
  const name = board.value;
  if (!f || !name || f.board !== name || boardStale.value) return [];
  return f.findings.map((x) => rowOfFinding(x, name));
});

/** Everything the dock can list. */
export const allRows = computed<Row[]>(() => [...projectRows.value, ...pcbRows.value]);

export const counts = computed(() => countRows(allRows.value));

/** What the section that has the keys is about, for "this view only". */
export const viewScope = computed<ViewScope>(() => {
  const vis = visible.value;
  const pane = vis.find((p) => p.index === layout.value.focus) ?? vis[0];
  return { section: pane?.section ?? 'schematic', file: activeFile.value, board: board.value };
});

/** The rows the dock shows: filtered, not yet grouped. */
export const listed = computed(() => filterRows(allRows.value, filter.value, viewScope.value));

/** The errors of the compiler, first at the top: what a stale view names. */
const errorRows = computed(() => sortRows(projectRows.value.filter((r) => r.severity === 'error')));

export const firstError = computed<Row | null>(() => errorRows.value[0] ?? null);

/** What the strip above a view that is not current says, or nothing when it is current. */
export const banner = computed<Banner | null>(() =>
  bannerFor({
    stale: boardStale.value,
    unsavedBroken: !boardStale.value && anyDirty.value && !!live.value && live.value.errors > 0,
    errors: errorRows.value,
  }),
);

/** The page has an error and no drawing at all: nothing built yet. */
export const neverBuilt = computed(() => (analysis.value?.boards.length ?? 0) === 0 && (analysis.value?.errors ?? 0) > 0);

// ---- what the PCB adds

type Extras = 'idle' | 'loading' | 'ready';
export const extras = signal<Extras>('idle');

async function loadExtras(name: string) {
  extras.value = 'loading';
  try {
    await loadFindings(name);
  } finally {
    extras.value = 'ready';
  }
}

// The PCB's findings are the server's to compute, and a page that is typing must not wait for
// them: they are asked for a moment after the analysis settles, once the dock is open or the PCB
// view has been shown. A board that cannot be placed (a power architecture of `Load` parts)
// answers with an error, which is the PCB view's to say, not a failure at every start. The power
// problems need no such call: the analysis carries them (`stage: "power"`).
effect(() => {
  const name = board.value;
  void analysis.value?.version;
  const withPcb = dock.value.open || scene.value?.board === name;
  if (!name || boardStale.value || !withPcb) return;
  const timer = setTimeout(() => void loadExtras(name), 500);
  return () => clearTimeout(timer);
});

// ---- what a row does

/** The instance path of the part whose declaration is the innermost to contain `at`. */
function partAt(at: Location): string | null {
  const parts = (detail.value?.parts ?? []).filter((p) => p.decl && covers(p.decl, at.file, at.line));
  parts.sort((a, b) => a.decl!.endLine - a.decl!.line - (b.decl!.endLine - b.decl!.line));
  return parts[0]?.path ?? null;
}

/** The PCB view is asked to zoom to a box, once. */
export const pcbFocus = signal<{ box: { x: number; y: number; width: number; height: number }; nonce: number } | null>(null);
let nonce = 0;

/**
 * THE ONE PLACE a row selects something in the views, through the page's shared selection: a
 * problem in the source selects the part that its line declares, a power issue its net. A PCB
 * finding also picks itself on the board and its parts there (the board keeps the whole group);
 * the shared selection is the part it names, when it names one.
 */
function selectFor(row: Row) {
  if (row.pcb) {
    pickedFinding.value = row.pcb.id;
    const sc = scene.value;
    const paths = sc && sc.board === row.board ? sc.parts.filter((p) => row.pcb!.parts.includes(p.refdes)).map((p) => p.path) : [];
    if (sc && sc.board === row.board) select(paths);
    // `select` told the page of a single part; of several it names none, so the first stands for them.
    if (paths.length > 1) pickShared(partSel(paths[0]), true);
    pcbFocus.value = { box: row.pcb.area, nonce: ++nonce };
    askFocus3d(row.pcb.parts, row.pcb.keepout, row.pcb.caseSolid, row.pcb.point3d);
    return;
  }
  // A power issue is about a rail or a stage: only a rail is a net.
  if (row.net && detail.value?.nets.some((n) => n.name === row.net)) {
    pickShared(netSel(row.net));
    return;
  }
  if (row.at) {
    const part = partAt(row.at);
    if (part) pickShared(partSel(part));
  }
}

/** Set the shared selection. `fromBoard`: the PCB view already shows it, with its own group of
 * parts and its own zoom. Otherwise the views are asked to bring it into view, as go to anything does. */
function pickShared(sel: Selection, fromBoard = false) {
  if (fromBoard) {
    wrote.sel = sel;
    setSelection(sel);
  } else {
    selectAndReveal(sel);
  }
}

/** A click on a row (or F8 landing on it): the thing is selected, and its place in the source is shown. */
export function revealRow(row: Row) {
  activeRow.value = row.id;
  selectFor(row);
  if (row.pcb) {
    revealSection('pcb');
    // The line that states what the finding quotes, put under the cursor where the source will
    // be when the person looks (the board is what they are on).
    if (row.at) void goTo(row.at, undefined, true);
  } else if (row.at) {
    void goTo(row.at);
  } else if (row.home === 'power') {
    revealSection('power');
  }
}

/** A click on a row's place (`src/main.nsm:12`): the source opens there. Not the row's own reveal, which also picks the thing the row is about
 * in the views (a PCB finding's parts on the board): the place is the place. */
export function revealPlace(row: Row) {
  const at = locationOf(row);
  if (!at) return;
  activeRow.value = row.id;
  void goTo(at);
}

/** F8 and Shift+F8: the next problem or the one before, from wherever the page is. */
export function stepProblem(dir: 1 | -1) {
  const list = steps(listed.value, group.value);
  const c = cursor.value;
  const here = viewScope.value.section === 'source' ? c : null;
  // In the source the cursor says where the person is: when it has left the line of the row that
  // was picked, the step goes from the cursor, as in VS Code.
  const picked = list.find((r) => r.id === activeRow.value);
  const moved = !!here && !!picked?.at && (picked.at.file !== here.file || picked.at.line !== here.line);
  const row = nextRow(list, moved ? null : activeRow.value, here, dir);
  if (!row) {
    notify({ title: 'No problems to go to', kind: 'info' });
    return;
  }
  revealRow(row);
}

/** The button of a stale view: the first error, in the source. */
export function goToFirstError() {
  const row = firstError.value;
  if (row) revealRow(row);
  else setDockOpen(true);
}
