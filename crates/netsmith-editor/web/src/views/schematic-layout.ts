// The layout file as the Schematic uses it (ADR 0061, stage 4): what the server read of it, and the one place that writes it.
//
// A write is the text of the whole file (`PUT /api/schematic-hints`), made from the file as the server last read it with one
// entry changed, so the entries of other boards and sheets are kept. Every write makes the server analyse the whole project again,
// so writes go one at a time (`OneAtATime`), one drop at a time, and a state says that the drawing is being redrawn. After a write
// the drawing comes back from the server's `rebuilt`, as it does after a save of the source.
//
// A drop that the drawing refuses (a finding `unusable`, "its block would lie on the part ...") is undone: the entry goes back to
// what it was, with a second write, and a toast names the reason. The block returns to where it was.

import { signal } from '@preact/signals';

import type { SchematicHints } from '../api';
import { ApiFailure, api, wasCancelled } from '../client';
import { notify } from '../feedback/state';
import { type HintAddress, type NoteEntry, type PartEntry, type Tables, tablesOf, toToml, withNote, withPart } from '../schematic/hints';
import { OneAtATime } from '../schematic/writes';
import { awaitRebuild, refreshAfterChange, say } from '../store';

/** What the server read of the layout file of the board on show, with the board it is for. */
export const hintsState = signal<{ board: string; data: SchematicHints } | null>(null);

/** Set while a write is on its way (the whole project is analysed again) and a second waits behind it. */
export const redrawing = signal<{ board: string; label: string; queued: boolean } | null>(null);

/** One change to the file, from the page. */
export interface Edit {
  /** The board as the editor names it (`TwoBoards/led`), for the routes. */
  board: string;
  /** The board's definition name, the file's table (`LedBoard`). */
  definition: string;
  /** What it is, for the words about it: `R1 moved`. */
  label: string;
  /** The tables after the change, from the tables as the file has them now; `null` writes nothing. */
  apply: (tables: Tables) => Tables | null;
  /** Looks at the drawing the write made (the SVG, parsed) and says why the change did nothing, or `null`: a block that holds the sheet's
   * reference part moves the whole drawing and no distance in it. The change is then undone, as a refusal is. */
  after?: (drawing: ParentNode) => string | null;
  /** The entry the change is about, to see whether the drawing refused it and to put it back: `before` is what it was. */
  entry?: { kind: 'part'; at: HintAddress; before: PartEntry | undefined } | { kind: 'note'; at: HintAddress; before: NoteEntry | undefined };
}

/** The line of the table header of the entry in what the server read, or null when it has none. */
export function entryLine(data: SchematicHints, definition: string, e: NonNullable<Edit['entry']>): number | null {
  const b = data.boards[definition];
  const t = e.at.sheet === null ? b : b?.sheets[e.at.sheet];
  const found = e.kind === 'part' ? t?.parts[e.at.key] : t?.notes[e.at.key];
  return found?.line ?? null;
}

/** The reason the drawing refused the entry at `line`, as the server words it, or null if it did not. */
export function refusalAt(data: SchematicHints, line: number | null): string | null {
  if (line === null) return null;
  const f = data.findings.find((x) => x.kind === 'unusable' && x.line === line);
  if (!f) return null;
  // `…: its block would lie on the part `x` at the place the file gives it; it is ignored`
  const own = `${f.entry}: `;
  return (f.message.startsWith(own) ? f.message.slice(own.length) : f.message).replace(/;?\s*it is ignored\.?$/, '');
}

export async function loadHints(board: string): Promise<SchematicHints | null> {
  try {
    const data = await api.schematicHints(board);
    hintsState.value = { board, data };
    return data;
  } catch {
    return null;
  }
}

/** Why a write was refused, in words for a toast. */
function why(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

async function perform(job: Edit): Promise<void> {
  let have = hintsState.peek();
  if (!have || have.board !== job.board) {
    const fresh = await loadHints(job.board);
    if (!fresh) {
      say('The layout file could not be read: the schematic was not changed.', true);
      return;
    }
    have = hintsState.peek();
  }
  if (!have) return;
  const { data } = have;
  if (data.errors.length > 0) {
    const first = data.errors[0];
    notify({
      kind: 'bad',
      title: `${data.file}:${first.line} does not read: ${first.message}`,
      detail: 'The schematic is drawn without the file. Fix it in the Source, then move parts.',
    });
    return;
  }
  const next = job.apply(tablesOf(data.boards));
  if (next === null) return;
  try {
    const answer = await api.writeSchematicHints(job.board, toToml(next), data.stamp);
    hintsState.value = { board: job.board, data: answer };
    await awaitRebuild();
    let refused = job.entry ? refusalAt(answer, entryLine(answer, job.definition, job.entry)) : null;
    let said = 'the drawing cannot put it there';
    if (!refused && job.after) {
      // Did the drawing change anything? A drawing the page cannot read says nothing: the write stands.
      const doc = await api.schematic(job.board).then((t) => new DOMParser().parseFromString(t, 'image/svg+xml'), () => null);
      refused = doc ? job.after(doc) : null;
      said = 'nothing was arranged';
    }
    if (refused && job.entry) {
      // The drawing could not honour it: the entry goes back to what it was, and the block with it.
      const back = job.entry.kind === 'part' ? withPart(next, job.definition, job.entry.at, job.entry.before ?? null) : withNote(next, job.definition, job.entry.at, job.entry.before ?? null);
      notify({ kind: 'bad', title: `${job.label}: ${said}`, detail: `${refused}. It is back where it was.` });
      if (!data.exists && Object.keys(back).length === 0) {
        // The drop made the file, and nothing is left in it: it goes, as it was not there before.
        await api.deletePath(answer.file, false);
        await refreshAfterChange();
        await loadHints(job.board);
      } else {
        const restored = await api.writeSchematicHints(job.board, toToml(back), answer.stamp);
        hintsState.value = { board: job.board, data: restored };
        await awaitRebuild();
      }
    }
  } catch (e) {
    if (e instanceof ApiFailure && e.status === 409) {
      notify({ kind: 'bad', title: `${data.file} changed on disk`, detail: 'The page read it again. Move the part again.' });
    } else if (e instanceof ApiFailure && e.status === 422) {
      notify({ kind: 'bad', title: `${job.label}: the layout file was not written`, detail: e.message });
    } else if (wasCancelled(e)) {
      say('The analysis was cancelled. The layout file is written: Run again redraws the schematic.');
    } else {
      notify({ kind: 'bad', title: `${job.label}: the layout file was not written`, detail: why(e) });
    }
    await loadHints(job.board);
  }
}

let label = '';
let board = '';
/** When the last write ended: a drawing that arrives soon after is the write's, and the view is kept steady across it. */
let lastWrite = 0;
export const lastWriteAt = () => lastWrite;

const queue = new OneAtATime<Edit>(
  (job) => {
    label = job.label;
    board = job.board;
    return perform(job).finally(() => {
      lastWrite = Date.now();
    });
  },
  () => {
    redrawing.value = queue.busy ? { board, label, queued: queue.queued } : null;
  },
);

/** Make a change to the file. A change that comes while one is being written waits for it (one only); more are refused, with a word. */
export function submitEdit(edit: Edit): 'started' | 'queued' | 'refused' {
  const result = queue.submit(edit);
  if (result === 'refused') say('Still redrawing: move it again in a moment.');
  return result;
}

/** Whether a write is on its way or waiting. */
export const writing = () => queue.busy;
export const full = () => queue.busy && queue.queued;
