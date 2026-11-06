// The comparison with a git ref, as signals: what the repository offers, the ref chosen, the answer
// and the state it is in, and the one switch that marks the other views (`showChanges`). The
// Compare section, the status bar's chip, the marks and the source's gutter all read this.

import { computed, effect, signal } from '@preact/signals';

import type { Compare, GitInfo, Location } from '../api';
import { ApiFailure, api } from '../client';
import { load, save } from '../storage';
import { analysis, board, say } from '../store';
import { type Category, type Scope, changeCount, marksOf } from './model';
import { beginCompareProgress, endCompareProgress } from './progress';
import { recallRef, refLabel, rememberRef, suggestedRef } from './refs';

/** What the repository offers. Null before it was asked. */
export const gitInfo = signal<GitInfo | null>(null);
export const gitFailure = signal<string | null>(null);

/** The ref chosen for this project (a name or a commit's hash), if any. */
export const refName = signal<string | null>(null);

export type CompareState =
  | { kind: 'idle' }
  | { kind: 'loading'; ref: string; since: number }
  | { kind: 'ready'; ref: string; data: Compare; /** A newer comparison is on its way. */ refreshing: boolean }
  | { kind: 'failed'; ref: string; message: string; notARef: boolean };

export const state = signal<CompareState>({ kind: 'idle' });

const MARKS_KEY = 'netsmith.compare.marks.v1';
/** Whether the other views carry the marks of the comparison. */
export const showChanges = signal<boolean>(load<boolean>(MARKS_KEY, false) === true);
effect(() => save(MARKS_KEY, showChanges.value));

/** The tab of the Compare section, the board in scope and how rows are grouped. */
export const tab = signal<Category>('parts');
export const scopeChoice = signal<Scope | null>(null);
export const pickerOpen = signal(false);

/** The comparison to draw, when there is one that can be: not one that failed, not one that is still on its way. */
export const compare = computed<Compare | null>(() => {
  const s = state.value;
  return s.kind === 'ready' && !s.data.error ? s.data : null;
});

/** The scope the section shows: what was chosen if the comparison has it, else the board in front, else all. */
export const scope = computed<Scope>(() => {
  const c = compare.value;
  const chosen = scopeChoice.value;
  if (!c) return 'all';
  if (chosen && (chosen === 'all' || c.boards.some((b) => b.name === chosen))) return chosen;
  const here = board.value;
  if (here && c.boards.some((b) => b.name === here)) return here;
  return c.boards.length === 1 ? c.boards[0].name : 'all';
});

/** The label of the chosen ref, for chips: `main`, `v1.2`, `4f2a9c1`. */
export const refText = computed(() => (refName.value ? refLabel(gitInfo.value, refName.value) : ''));

export const changes = computed(() => changeCount(compare.value));

/** The marks of the board in front, when the switch is on. */
export const marks = computed(() => {
  const c = compare.value;
  if (!showChanges.value || !c) return null;
  const name = board.value;
  return marksOf(c.boards.find((b) => b.name === name));
});

/** The comparison of the board in front, whatever the switch says. */
export const boardCompare = computed(() => compare.value?.boards.find((b) => b.name === board.value));

// ---- asking ----

let seq = 0;

/** What the repository offers. Asked when the section is first shown and when the chip is. */
export async function loadGit(): Promise<GitInfo | null> {
  try {
    const info = await api.git();
    gitInfo.value = info;
    gitFailure.value = null;
    if (info.available && refName.peek() === null) refName.value = recallRef(info);
    return info;
  } catch (e) {
    gitFailure.value = e instanceof Error ? e.message : String(e);
    return null;
  }
}

/** Compare the project with `ref`. A newer ask makes an older answer be dropped. */
export async function runCompare(ref: string, quiet = false): Promise<void> {
  const mine = ++seq;
  refName.value = ref;
  const prev = state.peek();
  if (quiet && prev.kind === 'ready') state.value = { ...prev, refreshing: true };
  else {
    state.value = { kind: 'loading', ref, since: Date.now() };
    beginCompareProgress(ref);
  }
  try {
    const data = await api.compare(ref);
    if (mine !== seq) return;
    state.value = { kind: 'ready', ref, data, refreshing: false };
    if (!data.error) rememberRef(gitInfo.peek(), ref);
  } catch (e) {
    if (mine !== seq) return;
    const failure = e instanceof ApiFailure ? e : null;
    state.value = { kind: 'failed', ref, message: e instanceof Error ? e.message : String(e), notARef: failure?.status === 404 || failure?.status === 400 };
  } finally {
    if (mine === seq) endCompareProgress();
  }
}

/** Choose a ref and compare with it. */
export function chooseRef(ref: string) {
  pickerOpen.value = false;
  baseTexts.clear();
  void runCompare(ref);
}

/** Forget the comparison (the ref no longer exists, or the person cleared it). */
export function clearCompare() {
  seq++;
  state.value = { kind: 'idle' };
  endCompareProgress();
  refName.value = null;
}

/** The ref to start from: what the project last used, else the repository's suggestion. */
export const startingRef = (): string | null => refName.peek() ?? recallRef(gitInfo.peek()) ?? suggestedRef(gitInfo.peek());

/** Turn the marks on or off. On, with no ref chosen, the person is asked for one. */
export function setShowChanges(on: boolean, askRef?: () => void) {
  if (on && !compare.peek()) {
    const ref = refName.peek() ?? recallRef(gitInfo.peek());
    if (ref) void runCompare(ref);
    else if (askRef) askRef();
    else say('Choose a ref to compare with first: the Compare section lists them.');
  }
  showChanges.value = on;
}

// The files on disk changed (a save, a rebuild): the comparison is made again, quietly, with the old answer on screen.
let lastVersion: number | undefined;
effect(() => {
  const v = analysis.value?.version;
  const s = state.peek();
  if (v !== undefined && lastVersion !== undefined && v !== lastVersion && s.kind === 'ready') void runCompare(s.ref, true);
  lastVersion = v;
});

// ---- the base side of a file ----

const baseTexts = new Map<string, string | null>();

/** A file of the project as it was at the compared commit; null when it did not exist there. */
export async function baseText(path: string): Promise<string | null> {
  const c = compare.peek();
  if (!c) return null;
  const key = `${c.base.hash}:${path}`;
  if (baseTexts.has(key)) return baseTexts.get(key)!;
  try {
    const f = await api.gitFile(c.base.hash, path);
    baseTexts.set(key, f.text);
    return f.text;
  } catch (e) {
    // A file that is new on the head side is not at the ref; any other failure is no base either.
    if (e instanceof ApiFailure && (e.status === 404 || e.status === 403)) {
      baseTexts.set(key, null);
      return null;
    }
    return null;
  }
}

// ---- the diff view ----

export interface DiffTarget {
  path: string;
  /** A line to bring into view, on the base side or the head's. */
  line?: number;
  side?: 'base' | 'head';
  /** Where it is open: over the Compare section's list, or over the editor of the Source section. */
  where: 'compare' | 'source';
}

export const diffTarget = signal<DiffTarget | null>(null);

export function openDiff(t: DiffTarget) {
  diffTarget.value = t;
}

export function closeDiff() {
  diffTarget.value = null;
}

/** The base side's location of something, as a diff target: the file and the line in it. */
export const baseLocation = (at: Location | undefined, where: DiffTarget['where'] = 'compare'): DiffTarget | null => (at ? { path: at.file, line: at.line, side: 'base', where } : null);
