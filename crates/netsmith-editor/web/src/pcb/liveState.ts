// What the page has been shown of the placer and the router while they run (issue 764), and of the runs it did not start
// (`fab --to-editor`): kept apart from the PCB store so that the socket's handler (`store.ts`) and the views share it.

import { signal } from '@preact/signals';

import type { OutsideProgress, PcbLive } from '../api';

/** The last glimpse of the placer of the board being placed, and of the router of the board being routed. */
export const livePlace = signal<PcbLive | null>(null);
export const liveRoute = signal<PcbLive | null>(null);
/** How many glimpses of each the page has been shown since it was opened: what a recipe reads (`data-live-place`). */
export const liveSeen = signal({ place: 0, route: 0 });
/** The last line the placer told while it places a board for the PCB view. */
export const placingSaid = signal<{ board: string; line: string } | null>(null);

export function notePcbLive(live: PcbLive) {
  if (live.task === 'place') livePlace.value = live;
  else if (live.task === 'route') liveRoute.value = live;
  else return;
  const seen = liveSeen.peek();
  liveSeen.value = { ...seen, [live.task]: seen[live.task as 'place' | 'route'] + 1 };
}

export function notePlacingLine(board: string, line: string) {
  placingSaid.value = { board, line };
}

/** A run the editor did not start, with the lines it told (the last few) and when the page first heard of it. */
export interface OutsideRun {
  run: string;
  command: string;
  board: string;
  lines: string[];
  started: number;
  ended?: { ok: boolean; at: number };
}

/** The runs from outside the page has heard of, the running ones and the last that ended, by `run`. */
export const outsideRuns = signal<Record<string, OutsideRun>>({});

export function noteOutside(p: OutsideProgress, now = Date.now()) {
  const runs = { ...outsideRuns.peek() };
  // A run that ended is kept only until another begins: the bar shows what runs, or how the last ended.
  if (!runs[p.run]) for (const [k, r] of Object.entries(runs)) if (r.ended) delete runs[k];
  const before = runs[p.run];
  runs[p.run] = {
    run: p.run,
    command: p.command,
    board: p.board || before?.board || '',
    lines: [...(before?.lines ?? []).slice(-19), p.message],
    started: before?.started ?? now,
    ended: p.ended ? { ok: p.ok ?? false, at: now } : before?.ended,
  };
  outsideRuns.value = runs;
}

/** Forget a run that ended (its item in the bar is closed). */
export function dropOutside(run: string) {
  const runs = { ...outsideRuns.peek() };
  delete runs[run];
  outsideRuns.value = runs;
}
