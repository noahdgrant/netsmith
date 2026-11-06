import { describe, expect, it } from 'vitest';

import { allRefs, dirtyText, findRef, projectKey, recallRef, refGroups, refLabel, relativeDate, rememberRef, suggestedRef } from './refs';
import { info } from './testdata';

/** A storage that holds text, or breaks. */
function store(initial?: string, broken?: 'read' | 'write' | 'both') {
  const data = new Map<string, string>();
  if (initial !== undefined) data.set('netsmith.compare.ref.v1', initial);
  return {
    getItem: (k: string) => {
      if (broken === 'read' || broken === 'both') throw new Error('blocked');
      return data.get(k) ?? null;
    },
    setItem: (k: string, v: string) => {
      if (broken === 'write' || broken === 'both') throw new Error('full');
      data.set(k, v);
    },
    data,
  };
}

describe('the picker lists', () => {
  it('start with HEAD, the upstream and the default branch, then branches, tags and commits', () => {
    const g = refGroups(info());
    expect(g.map((x) => x.id)).toEqual(['quick', 'branches', 'tags', 'commits']);
    expect(g[0].items.map((i) => i.name)).toEqual(['HEAD', 'origin/main', 'main']);
    expect(g[1].items.map((i) => i.name)).toEqual(['feature', 'broken']);
    expect(g[2].items.map((i) => i.name)).toEqual(['v1.0']);
    expect(g[3].items.map((i) => i.kind)).toEqual(['commit', 'commit', 'commit']);
    // a commit is asked for by its hash and read by its short one
    expect(g[3].items[0].name).toHaveLength(40);
    expect(g[3].items[0].label).toHaveLength(7);
  });
  it('lists a branch once: the default branch is not among the branches again', () => {
    const names = allRefs(info()).filter((r) => r.kind !== 'commit').map((r) => r.name);
    expect(names.filter((n) => n === 'main')).toHaveLength(1);
  });
  it('marks the branch that is checked out', () => {
    expect(allRefs(info()).find((r) => r.name === 'feature')?.current).toBe(true);
    expect(allRefs(info()).find((r) => r.name === 'broken')?.current).toBe(false);
  });
  it('are filtered by every word of a search, in the name, the subject, the author and the hash', () => {
    expect(refGroups(info(), 'v1').flatMap((g) => g.items).map((i) => i.name)).toEqual(['v1.0']);
    expect(refGroups(info(), 'tune led').flatMap((g) => g.items).every((i) => /tune the led/i.test(i.subject))).toBe(true);
    expect(refGroups(info(), 'ada second').flatMap((g) => g.items)).toHaveLength(1);
    expect(refGroups(info(), '1111111').flatMap((g) => g.items).length).toBeGreaterThan(0);
    expect(refGroups(info(), 'nothing like this')).toEqual([]);
    // a group with nothing left is not shown
    expect(refGroups(info(), 'broken').map((g) => g.id)).toEqual(['branches']);
  });
  it('are empty for no repository', () => {
    expect(refGroups(null)).toEqual([]);
    expect(refGroups(info({ available: false, refs: [], commits: [] }))).toEqual([]);
  });
  it('know a ref by its name, and show a hash as seven characters', () => {
    expect(refLabel(info(), 'main')).toBe('main');
    expect(refLabel(info(), '86d3736cc7b9f7660d36b29acc324b12ee90fd7b')).toBe('86d3736');
    expect(refLabel(info(), 'some-other')).toBe('some-other');
    expect(findRef(info(), 'v1.0')?.kind).toBe('tag');
    expect(findRef(info(), 'zzz')).toBeUndefined();
  });
  it('suggest the upstream, else the default branch, else HEAD', () => {
    expect(suggestedRef(info())).toBe('origin/main');
    expect(suggestedRef(info({ refs: info().refs.filter((r) => r.kind !== 'upstream') }))).toBe('main');
    expect(suggestedRef(info({ refs: info().refs.filter((r) => r.kind === 'head') }))).toBe('HEAD');
    expect(suggestedRef(null)).toBeNull();
  });
});

describe('the sentence about the work tree', () => {
  it('says whether what is compared is what was committed', () => {
    expect(dirtyText(info())).toBe('The working tree has uncommitted changes (compared as is).');
    expect(dirtyText(info({ projectDirty: false, dirty: false }))).toMatch(/clean/);
    expect(dirtyText(info({ projectDirty: false, dirty: true }))).toMatch(/outside the project/);
    expect(dirtyText(null)).toBe('');
  });
});

describe('relative dates', () => {
  const now = Date.parse('2026-11-06T12:00:00Z');
  it('reads like a person would say it', () => {
    expect(relativeDate('2026-11-06T11:59:50Z', now)).toBe('just now');
    expect(relativeDate('2026-11-06T11:30:00Z', now)).toBe('30 min ago');
    expect(relativeDate('2026-11-06T07:00:00Z', now)).toBe('5 h ago');
    expect(relativeDate('2026-11-03T12:00:00Z', now)).toBe('3 d ago');
    expect(relativeDate('2026-10-16T12:00:00Z', now)).toBe('3 wk ago');
    expect(relativeDate('2026-06-06T12:00:00Z', now)).toBe('5 mo ago');
    expect(relativeDate('2024-11-06T12:00:00Z', now)).toBe('2 y ago');
  });
  it('is empty for what is not a date, and never negative', () => {
    expect(relativeDate('yesterday', now)).toBe('');
    expect(relativeDate('2027-01-01T00:00:00Z', now)).toBe('just now');
  });
});

describe('the ref remembered for a project', () => {
  it('is kept for each project by where it is in the work tree', () => {
    const s = store();
    expect(rememberRef(info(), 'main', s)).toBe(true);
    expect(rememberRef(info({ projectDir: 'other' }), 'v1.0', s)).toBe(true);
    expect(recallRef(info(), s)).toBe('main');
    expect(recallRef(info({ projectDir: 'other' }), s)).toBe('v1.0');
    expect(projectKey(info())).toBe('/work/board/hardware');
  });
  it('is forgotten when the repository no longer has it, but a commit hash is kept', () => {
    const s = store();
    rememberRef(info(), 'deleted-branch', s);
    expect(recallRef(info(), s)).toBeNull();
    rememberRef(info(), '86d3736cc7b9f7660d36b29acc324b12ee90fd7b', s);
    expect(recallRef(info(), s)).toBe('86d3736cc7b9f7660d36b29acc324b12ee90fd7b');
  });
  it('falls back when the storage is missing, throws or holds rubbish', () => {
    expect(recallRef(info(), null)).toBeNull();
    expect(rememberRef(info(), 'main', null)).toBe(false);
    expect(recallRef(info(), store(undefined, 'read'))).toBeNull();
    expect(rememberRef(info(), 'main', store(undefined, 'write'))).toBe(false);
    expect(rememberRef(info(), 'main', store(undefined, 'both'))).toBe(false);
    for (const junk of ['not json', '[1,2]', '3', 'null', '{"x":3}']) expect(recallRef(info(), store(junk))).toBeNull();
  });
  it('does not lose another project when it keeps one', () => {
    const s = store(JSON.stringify({ '/elsewhere/': 'main' }));
    rememberRef(info(), 'v1.0', s);
    expect(JSON.parse(s.data.get('netsmith.compare.ref.v1')!)).toEqual({ '/elsewhere/': 'main', '/work/board/hardware': 'v1.0' });
  });
});
