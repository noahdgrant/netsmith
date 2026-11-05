// What the dock and the status bar list, read from the store: the compiler's problems, the PCB's
// findings and the power tree's issues as one list of rows; and what a click or F8 on a row does.

import { computed, effect, signal } from '@preact/signals';

import type { Location } from '../api';
import { api } from '../client';
import { modelOf } from '../power/model';
import { findings, loadFindings, pickedFinding, scene, select, wrote } from '../pcbstore';
import { type Selection, netSel, partSel, setSelection } from '../selection';
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
  systemName,
  visible,
} from '../store';
import { covers } from '../util';
import { activeRow, filter, group, notify, setDockOpen } from './state';
import {
  type Row,
  type ViewScope,
  countRows,
  filterRows,
  nextRow,
  rowOfFinding,
  rowOfPowerIssue,
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

/** The issues of the power tree, as far as the dock has asked for them. */
const power = signal<{ key: string; rows: Row[] } | null>(null);
const powerKey = computed(() => `${systemName.value ?? board.value}|${analysis.value?.version}`);
const powerRows = computed<Row[]>(() => (power.value && power.value.key === powerKey.value ? power.value.rows : []));

/** Everything the dock can list. */
export const allRows = computed<Row[]>(() => [...projectRows.value, ...pcbRows.value, ...powerRows.value]);

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

// ---- what PCB and power add

type Extras = 'idle' | 'loading' | 'ready';
export const extras = signal<Extras>('idle');

async function loadExtras(name: string) {
  extras.value = 'loading';
  const key = powerKey.peek();
  const target = systemName.peek() ?? name;
  try {
    await Promise.all([
      loadFindings(name),
      api
        .power(target)
        .then((tree) => {
          const model = modelOf(tree);
          const rows = model.issues.map((issue) => {
            const node = tree.nodes.find((n) => n.id === issue.id);
            const at = node?.loads.find((l) => l.at)?.at ?? node?.supplies.find((l) => l.at)?.at ?? null;
            return rowOfPowerIssue(issue, name, at);
          });
          power.value = { key, rows };
        })
        .catch(() => {
          power.value = { key, rows: [] };
        }),
    ]);
  } finally {
    extras.value = 'ready';
  }
}

// The PCB and the power tree are asked for a moment after the analysis settles, so the counts of
// the status bar are the same whether or not the dock is open. They are the server's to compute,
// and a page that is typing must not wait for them: only a read of the saved files asks again.
effect(() => {
  const name = board.value;
  void powerKey.value;
  if (!name || boardStale.value) return;
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

/** Set the shared selection; `fromBoard` says the PCB view already shows it (its own group of parts). */
function pickShared(sel: Selection, fromBoard = false) {
  if (fromBoard) wrote.sel = sel;
  setSelection(sel);
}

/** A click on a row (or F8 landing on it): the thing is selected, and its place in the source is shown. */
export function revealRow(row: Row) {
  activeRow.value = row.id;
  selectFor(row);
  if (row.pcb) {
    revealSection('pcb');
  } else if (row.at) {
    void goTo(row.at);
  } else if (row.home === 'power') {
    revealSection('power');
  }
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
