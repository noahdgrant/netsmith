// The refs a comparison can start from (`GET /api/git`), put in the order the picker lists them,
// found by what a person types, and remembered for each project. Pure but for the browser's storage,
// which a function takes as an argument so that a test can break it.

import type { GitCommit, GitInfo, GitRef, GitRefKind } from '../api';

export type RefKind = GitRefKind | 'commit';

export interface RefItem {
  /** What to pass as `ref`: a name, or the hash of a commit. */
  name: string;
  kind: RefKind;
  /** What the picker shows as the name: `main`, `v1.2`, `4f2a9c1`. */
  label: string;
  hash: string;
  short: string;
  subject: string;
  date: string;
  author?: string;
  /** The branch that is checked out. */
  current: boolean;
}

export interface RefGroup {
  id: 'quick' | 'branches' | 'tags' | 'commits';
  label: string;
  items: RefItem[];
}

const fromRef = (r: GitRef): RefItem => ({ name: r.name, kind: r.kind, label: r.name, hash: r.hash, short: r.short, subject: r.subject, date: r.date, current: r.current });
const fromCommit = (c: GitCommit): RefItem => ({ name: c.hash, kind: 'commit', label: c.short, hash: c.hash, short: c.short, subject: c.subject, date: c.date, author: c.author, current: false });

/** The words of a query, each of which must be in a ref's name, subject, author or hash. */
function matches(item: RefItem, words: string[]): boolean {
  if (words.length === 0) return true;
  const hay = `${item.label} ${item.name} ${item.subject} ${item.author ?? ''} ${item.hash}`.toLowerCase();
  return words.every((w) => hay.includes(w));
}

/** The picker's lists, in order: HEAD, the upstream and the default branch; the branches; the tags;
 * the recent commits. A ref that the quick list has is not listed again among the branches. */
export function refGroups(info: GitInfo | null, query = ''): RefGroup[] {
  if (!info?.available) return [];
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const kinds = (...k: GitRefKind[]) => info.refs.filter((r) => k.includes(r.kind)).map(fromRef);
  const quick = kinds('head', 'upstream', 'default');
  const quickNames = new Set(quick.map((r) => r.name));
  const groups: RefGroup[] = [
    { id: 'quick', label: 'Start from', items: quick },
    { id: 'branches', label: 'Branches', items: kinds('branch').filter((r) => !quickNames.has(r.name)) },
    { id: 'tags', label: 'Tags', items: kinds('tag') },
    { id: 'commits', label: 'Recent commits', items: info.commits.map(fromCommit) },
  ];
  return groups.map((g) => ({ ...g, items: g.items.filter((i) => matches(i, words)) })).filter((g) => g.items.length > 0);
}

/** Every ref the picker could show, flat, in the same order. */
export const allRefs = (info: GitInfo | null): RefItem[] => refGroups(info).flatMap((g) => g.items);

/** A ref as a person reads it in a chip: a name, or the short hash of a commit. */
export function refLabel(info: GitInfo | null, ref: string): string {
  const hit = allRefs(info).find((r) => r.name === ref);
  if (hit) return hit.label;
  return /^[0-9a-f]{12,40}$/i.test(ref) ? ref.slice(0, 7) : ref;
}

/** The item for a ref name, if the repository lists it. */
export const findRef = (info: GitInfo | null, ref: string): RefItem | undefined => allRefs(info).find((r) => r.name === ref);

/** `3 min ago` for an ISO date; an empty string for one that is not a date. */
export function relativeDate(iso: string, now: number): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return '';
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 45) return 'just now';
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  if (d < 14) return `${d} d ago`;
  if (d < 60) return `${Math.round(d / 7)} wk ago`;
  if (d < 365) return `${Math.round(d / 30)} mo ago`;
  return `${Math.round(d / 365)} y ago`;
}

/** The sentence about the work tree: whether what is compared is what was committed. */
export function dirtyText(info: GitInfo | null): string {
  if (!info?.available) return '';
  return info.projectDirty
    ? 'The working tree has uncommitted changes (compared as is).'
    : info.dirty
      ? 'Files outside the project have uncommitted changes; the project is as committed.'
      : 'The working tree is clean: HEAD is what is on disk.';
}

// ---- remembered per project ----

const KEY = 'netsmith.compare.ref.v1';

/** The key of a project: where it is in the work tree. */
export const projectKey = (info: GitInfo | null): string => `${info?.root ?? ''}/${info?.projectDir ?? ''}`;

type Reader = Pick<Storage, 'getItem'> | null;
type Writer = Pick<Storage, 'setItem' | 'getItem'> | null;

function browser(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

function readAll(store: Reader): Record<string, string> {
  try {
    const text = store?.getItem(KEY);
    if (!text) return {};
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {};
    return Object.fromEntries(Object.entries(parsed).filter(([, v]) => typeof v === 'string')) as Record<string, string>;
  } catch {
    return {};
  }
}

/** The ref this project was last compared with, when the repository still has it; else null.
 * Storage that is missing, throws or holds rubbish gives null, never an error. */
export function recallRef(info: GitInfo | null, store: Reader = browser()): string | null {
  const ref = readAll(store)[projectKey(info)];
  if (!ref) return null;
  return findRef(info, ref) || /^[0-9a-f]{7,40}$/i.test(ref) ? ref : null;
}

/** Keep `ref` as this project's. False when it could not be kept (the page runs without it). */
export function rememberRef(info: GitInfo | null, ref: string, store: Writer = browser()): boolean {
  try {
    if (!store) return false;
    store.setItem(KEY, JSON.stringify({ ...readAll(store), [projectKey(info)]: ref }));
    return true;
  } catch {
    return false;
  }
}

/** A good ref to offer first when none was remembered: the default branch, else the upstream, else HEAD. */
export function suggestedRef(info: GitInfo | null): string | null {
  if (!info?.available) return null;
  const by = (k: GitRefKind) => info.refs.find((r) => r.kind === k)?.name;
  return by('upstream') ?? by('default') ?? by('head') ?? null;
}
