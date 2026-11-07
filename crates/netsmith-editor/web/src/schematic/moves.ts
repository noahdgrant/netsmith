// The changes the Schematic can make to the layout file, as pure functions: a drop, a nudge, a turn, a mirror, a clear. Each makes an
// `Edit` (`views/schematic-layout.ts`) from what the drawing says (`blocks.ts`) and what the server read of the file. None of them
// touches the DOM or the network, so each is tested alone.

import type { SchematicHints } from '../api';
import type { Edit } from '../views/schematic-layout';
import { type NoteFacts, type PartFacts, hintSheet, noteKeyOf, pinFrame } from './blocks';
import { movedEntry, mirroredEntry, turnedEntry } from './drag';
import {
  type HintAddress,
  type PartEntry,
  type Tables,
  countAll,
  countIn,
  mmToSteps,
  noteEntry,
  partEntry,
  stepsToMm,
  tablesOf,
  withNote,
  withPart,
  withoutSheet,
} from './hints';

/** What a change is made from. */
export interface Ctx {
  /** The board as the editor names it, and its definition (the file's table). */
  board: string;
  definition: string;
  /** What the server read of the file. */
  data: SchematicHints;
  /** The parts as the drawing has them, by designator. */
  facts: Map<string, PartFacts>;
  /** The module instances of the board (the sheets that are not the board's own). */
  modules: ReadonlySet<string>;
}

/** The entries as the file has them. */
export const tablesOfCtx = (ctx: Ctx): Tables => tablesOf(ctx.data.boards);

/** The lines of the entries the drawing refused: what is drawn for them is not what they say. */
export function refusedLines(data: SchematicHints): Set<number> {
  return new Set(data.findings.filter((f) => f.kind === 'unusable').map((f) => f.line));
}

/** Whether the file can be written at all: it reads. A file with a fault is ignored as a whole, and writing would drop it. */
export const writable = (data: SchematicHints | undefined | null): boolean => !!data && data.errors.length === 0;

/** Why a part cannot be dragged, or null if it can. */
export function dragRefusal(ctx: Ctx, ref: string): string | null {
  const f = ctx.facts.get(ref);
  if (!f) return 'The drawing has no such part.';
  if (!writable(ctx.data)) return `${ctx.data.file} has an error and is ignored: fix it in the Source first.`;
  if (f.anchor) return "The sheet's anchor: drag a neighbour.";
  return null;
}

const lineOf = (ctx: Ctx, at: HintAddress): number | undefined => {
  const b = ctx.data.boards[ctx.definition];
  const t = at.sheet === null ? b : b?.sheets[at.sheet];
  return t?.parts[at.key]?.line;
};

/** The entry as the file has it, with the line it is on (for `honoured`). */
function currentEntry(ctx: Ctx, f: PartFacts): { entry: PartEntry | undefined; honoured: boolean } {
  const tables = tablesOfCtx(ctx);
  const entry = partEntry(tables, ctx.definition, f.address);
  const line = lineOf(ctx, f.address);
  return { entry, honoured: !(line !== undefined && refusedLines(ctx.data).has(line)) };
}

/** Whether the part has an entry, and whether the drawing refused it (it is drawn as if it had none). */
export function entryState(ctx: Ctx, f: PartFacts): { has: boolean; refused: boolean } {
  const { entry, honoured } = currentEntry(ctx, f);
  return { has: entry !== undefined, refused: entry !== undefined && !honoured };
}

function partEdit(ctx: Ctx, f: PartFacts, label: string, change: (entry: PartEntry | undefined, ok: boolean) => PartEntry | null): Edit {
  const { entry: before } = currentEntry(ctx, f);
  return {
    board: ctx.board,
    definition: ctx.definition,
    label,
    entry: { kind: 'part', at: f.address, before },
    apply: (tables) => {
      const now = partEntry(tables, ctx.definition, f.address);
      // An entry the drawing refused is not what is drawn: a move starts from the engine's place.
      const ok = currentEntry(ctx, f).honoured;
      return withPart(tables, ctx.definition, f.address, change(now, ok));
    },
  };
}

/** A part's block was dropped `sx`, `sy` grid steps from where it is drawn. */
export function dropEdit(ctx: Ctx, ref: string, sx: number, sy: number, pin: boolean): Edit | null {
  const f = ctx.facts.get(ref);
  if (!f || dragRefusal(ctx, ref)) return null;
  const frame = pin ? pinFrame(ctx.facts, f, (a) => currentEntry(ctx, a)) : null;
  return partEdit(ctx, f, `${ref} ${pin && frame ? 'pinned' : 'moved'}`, (e, ok) => movedEntry(e, sx, sy, pin, frame, ok));
}

/** An arrow key: the block of the selected part is moved by `sx`, `sy` grid steps. */
export function nudgeEdit(ctx: Ctx, ref: string, sx: number, sy: number): Edit | null {
  const f = ctx.facts.get(ref);
  if (!f || dragRefusal(ctx, ref)) return null;
  return partEdit(ctx, f, `${ref} moved`, (e, ok) => movedEntry(e, sx, sy, false, null, ok));
}

/** A quarter turn counterclockwise (`dir` 1) or clockwise (-1), from the orientation the part is drawn in. */
export function turnEdit(ctx: Ctx, ref: string, dir: 1 | -1): Edit | null {
  const f = ctx.facts.get(ref);
  if (!f?.orient || !writable(ctx.data)) return null;
  const drawn = f.orient;
  return partEdit(ctx, f, `${ref} turned`, (e) => turnedEntry(e, drawn, dir));
}

export function mirrorEdit(ctx: Ctx, ref: string): Edit | null {
  const f = ctx.facts.get(ref);
  if (!f?.orient || !writable(ctx.data)) return null;
  const drawn = f.orient;
  return partEdit(ctx, f, `${ref} mirrored`, (e) => mirroredEntry(e, drawn));
}

/** Whether the part has an entry in the file. */
export function hasEntry(ctx: Ctx, ref: string): boolean {
  const f = ctx.facts.get(ref);
  return !!f && partEntry(tablesOfCtx(ctx), ctx.definition, f.address) !== undefined;
}

/** The entry of the part is removed: the block goes back to where the drawing puts it. */
export function clearPartEdit(ctx: Ctx, ref: string): Edit | null {
  const f = ctx.facts.get(ref);
  if (!f || !writable(ctx.data) || !hasEntry(ctx, ref)) return null;
  return partEdit(ctx, f, `${ref} back in its place`, () => null);
}

/** A note's frame was dropped `sx`, `sy` grid steps from where it is drawn. */
export function noteDropEdit(ctx: Ctx, note: NoteFacts, sx: number, sy: number): Edit | null {
  if (!writable(ctx.data)) return null;
  const sheet = hintSheet(note.slot, ctx.modules);
  const key = noteKeyOf(note, sheet);
  const at: HintAddress = { sheet, key };
  const before = noteEntry(tablesOfCtx(ctx), ctx.definition, at);
  return {
    board: ctx.board,
    definition: ctx.definition,
    label: 'The note moved',
    entry: { kind: 'note', at, before },
    apply: (tables) => {
      const now = noteEntry(tables, ctx.definition, at);
      return withNote(tables, ctx.definition, at, {
        dx: stepsToMm(mmToSteps(now?.dx ?? 0) + sx),
        dy: stepsToMm(mmToSteps(now?.dy ?? 0) + sy),
      });
    },
  };
}

/** Every entry of the sheet (`null`: the board's own) is removed. */
export function clearSheetEdit(ctx: Ctx, sheet: string | null): Edit | null {
  if (!writable(ctx.data) || countIn(tablesOfCtx(ctx), ctx.definition, sheet) === 0) return null;
  return { board: ctx.board, definition: ctx.definition, label: 'The layout of the module cleared', apply: (t) => withoutSheet(t, ctx.definition, sheet) };
}

/** Every entry of the file, in every board, is removed (the file keeps `version = 1`). */
export function clearAllEdit(ctx: Ctx): Edit | null {
  if (!writable(ctx.data) || countAll(tablesOfCtx(ctx)) === 0) return null;
  return { board: ctx.board, definition: ctx.definition, label: 'The layout cleared', apply: () => ({}) };
}
