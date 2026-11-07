import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { board } from '../store';
import { againstName, chooseAgainst, chooseRef, clearCompare, compare, changes, baseText, diskCompare, gitInfo, headTextAtRef, loadGit, marks, pairText, refName, runCompare, scope, scopeChoice, setShowChanges, showChanges, sinceText, state } from './store';
import { changed, info } from './testdata';

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

/** A server that answers by path; `compare` can be held back by a promise. */
function server(routes: Record<string, (url: URL) => Response | Promise<Response>>) {
  const fetchMock = vi.fn(async (input: string | URL | Request) => {
    const url = new URL(String(input), 'http://x');
    const route = routes[url.pathname];
    if (!route) return new Response('no', { status: 404 });
    return route(url);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

beforeEach(() => {
  clearCompare();
  gitInfo.value = null;
  scopeChoice.value = null;
  showChanges.value = false;
  board.value = null;
});
afterEach(() => vi.unstubAllGlobals());

describe('two refs compared with each other', () => {
  const two = () => ({ ...changed(), against: 'feature', head: { commit: { hash: 'f'.repeat(40), short: 'ffffff0', subject: 'the feature', author: 'A', date: '2026-11-05T10:00:00+01:00' }, dirty: false, errors: 0 } });
  it('asks the server for `against`, and says the pair in its words', async () => {
    const seen: string[] = [];
    server({
      '/api/compare': (url) => {
        seen.push(url.search);
        return json(url.searchParams.get('against') ? two() : changed());
      },
    });
    await runCompare('main', false, 'feature');
    expect(seen[0]).toContain('ref=main');
    expect(seen[0]).toContain('against=feature');
    expect(againstName.value).toBe('feature');
    expect(pairText.value).toBe('main and feature');
    expect(sinceText.value).toBe('between main and feature');
    // A ref against the working tree sends no `against`.
    await runCompare('main', false, null);
    expect(seen[1]).not.toContain('against');
    expect(againstName.value).toBeNull();
    expect(sinceText.value).toBe('since main');
  });
  it('marks no view: the views draw the files on disk', async () => {
    server({ '/api/compare': () => json(two()) });
    board.value = 'TwoBoards/led';
    showChanges.value = true;
    await runCompare('main', false, 'feature');
    expect(compare.value).not.toBeNull();
    expect(diskCompare.value).toBeNull();
    expect(marks.value).toBeNull();
    // Back against the working tree the marks are there.
    server({ '/api/compare': () => json(changed()) });
    await runCompare('main', false, null);
    expect(diskCompare.value).not.toBeNull();
    expect(marks.value?.parts.get('r1')).toBe('changed');
  });
  it('turning the marks on while two refs are compared says why it cannot', async () => {
    server({ '/api/compare': () => json(two()) });
    await runCompare('main', false, 'feature');
    setShowChanges(true);
    expect(showChanges.value).toBe(false);
  });
  it('choosing the second ref keeps the first, and `null` goes back to the working tree', async () => {
    const seen: string[] = [];
    server({ '/api/compare': (url) => (seen.push(url.search), json(url.searchParams.get('against') ? two() : changed())) });
    await runCompare('main');
    chooseAgainst('feature');
    await vi.waitFor(() => expect(state.value.kind).toBe('ready'));
    await vi.waitFor(() => expect(againstName.value).toBe('feature'));
    expect(seen.at(-1)).toContain('ref=main');
    expect(seen.at(-1)).toContain('against=feature');
    chooseRef('v1.0');
    await vi.waitFor(() => expect(seen.at(-1)).toContain('ref=v1.0'));
    expect(seen.at(-1)).toContain('against=feature');
    chooseAgainst(null);
    await vi.waitFor(() => expect(againstName.value).toBeNull());
    expect(seen.at(-1)).not.toContain('against');
  });
  it('reads the head side of a file at the second ref, and nothing for the working tree', async () => {
    const files: string[] = [];
    server({
      '/api/compare': () => json(two()),
      '/api/git/file': (url) => (files.push(url.searchParams.get('ref')!), json({ path: 'src/main.nsm', text: 'x', stamp: 'f', readonly: true })),
    });
    await runCompare('main', false, 'feature');
    expect(await headTextAtRef('src/main.nsm')).toBe('x');
    expect(files).toEqual(['f'.repeat(40)]);
    server({ '/api/compare': () => json(changed()) });
    await runCompare('main', false, null);
    expect(await headTextAtRef('src/main.nsm')).toBeUndefined();
  });
  it('is forgotten with the comparison', async () => {
    server({ '/api/compare': () => json(two()) });
    await runCompare('main', false, 'feature');
    clearCompare();
    expect(againstName.value).toBeNull();
    expect(refName.value).toBeNull();
  });
});

describe('asking for a comparison', () => {
  it('is loading, then ready with the answer, the changes and the marks of the board in front', async () => {
    server({ '/api/compare': () => json(changed()) });
    board.value = 'TwoBoards/power';
    const done = runCompare('main');
    expect(state.value).toMatchObject({ kind: 'loading', ref: 'main' });
    await done;
    expect(state.value.kind).toBe('ready');
    expect(compare.value?.summary).toMatch(/1 part added/);
    expect(refName.value).toBe('main');
    expect(changes.value).toBe(12);
    // the marks are of the board in front, and only while the switch is on
    expect(marks.value).toBeNull();
    showChanges.value = true;
    expect(marks.value?.parts.get('stage.bulk')).toBe('added');
    board.value = 'TwoBoards/led';
    expect(marks.value?.parts.get('r1')).toBe('changed');
    expect(marks.value?.parts.has('stage.bulk')).toBe(false);
  });
  it('a ref that does not analyse is an answer: ready, with an error, and nothing to draw marks from', async () => {
    server({ '/api/compare': () => json({ ...changed(), error: 'the project does not analyse at 54e1638', boards: [], baseProblems: [] }) });
    await runCompare('broken');
    expect(state.value.kind).toBe('ready');
    expect(compare.value).toBeNull();
    showChanges.value = true;
    expect(marks.value).toBeNull();
    expect(changes.value).toBe(0);
  });
  it('a name that is no ref is a failure that says so', async () => {
    server({ '/api/compare': () => new Response('no commit called nope', { status: 404 }) });
    await runCompare('nope');
    expect(state.value).toMatchObject({ kind: 'failed', ref: 'nope', notARef: true });
    server({ '/api/compare': () => new Response('git failed', { status: 500 }) });
    await runCompare('main');
    expect(state.value).toMatchObject({ kind: 'failed', notARef: false });
  });
  it('the answer to the newest ask is the one kept, whichever comes last', async () => {
    let release: (r: Response) => void = () => {};
    const slow = new Promise<Response>((r) => (release = r));
    let n = 0;
    server({ '/api/compare': () => (n++ === 0 ? slow : json({ ...changed(), ref: 'v1.0' })) });
    const first = runCompare('main');
    await runCompare('v1.0');
    release(json({ ...changed(), ref: 'main' }));
    await first;
    expect(state.value).toMatchObject({ kind: 'ready', ref: 'v1.0' });
    expect(compare.value?.ref).toBe('v1.0');
  });
  it('a quiet refresh keeps the old answer on screen while the new one is on its way', async () => {
    server({ '/api/compare': () => json(changed()) });
    await runCompare('main');
    let release: (r: Response) => void = () => {};
    server({ '/api/compare': () => new Promise<Response>((r) => (release = r)) });
    const again = runCompare('main', true);
    expect(state.value).toMatchObject({ kind: 'ready', refreshing: true });
    expect(compare.value).not.toBeNull();
    release(json(changed()));
    await again;
    expect(state.value).toMatchObject({ kind: 'ready', refreshing: false });
  });
  it('choosing a ref closes the picker and compares', async () => {
    server({ '/api/compare': () => json(changed()) });
    chooseRef('main');
    expect(state.value.kind).toBe('loading');
    await vi.waitFor(() => expect(state.value.kind).toBe('ready'));
  });
  it('stopping forgets the ref and the answer', async () => {
    server({ '/api/compare': () => json(changed()) });
    await runCompare('main');
    clearCompare();
    expect(state.value.kind).toBe('idle');
    expect(refName.value).toBeNull();
    expect(compare.value).toBeNull();
  });
});

describe('the board a list is about', () => {
  beforeEach(async () => {
    server({ '/api/compare': () => json(changed()) });
    await runCompare('main');
  });
  it('is all the boards, whichever board is in front, until one is chosen in the section', () => {
    board.value = 'TwoBoards/led';
    expect(scope.value).toBe('all');
    board.value = 'Other/board';
    expect(scope.value).toBe('all');
    board.value = null;
    expect(scope.value).toBe('all');
  });
  it('is the board of a project that has only one', async () => {
    const one = changed();
    one.boards = [one.boards[0]];
    server({ '/api/compare': () => json(one) });
    await runCompare('main');
    expect(scope.value).toBe(one.boards[0].name);
  });
  it('is what was chosen, when the comparison has it', () => {
    board.value = 'TwoBoards/led';
    scopeChoice.value = 'TwoBoards/power';
    expect(scope.value).toBe('TwoBoards/power');
    scopeChoice.value = 'all';
    expect(scope.value).toBe('all');
    scopeChoice.value = 'Gone/board';
    expect(scope.value).toBe('all');
  });
});

describe('the switch', () => {
  it('turning it on with no ref asks for one', async () => {
    server({ '/api/git': () => json(info()) });
    const ask = vi.fn();
    setShowChanges(true, ask);
    expect(showChanges.value).toBe(true);
    await vi.waitFor(() => expect(ask).toHaveBeenCalledOnce());
  });
  it('turning it on in a project with no repository says so and asks for nothing', async () => {
    server({ '/api/git': () => json(info({ available: false, refs: [], commits: [], reason: 'not a work tree' })) });
    const ask = vi.fn();
    setShowChanges(true, ask);
    await vi.waitFor(() => expect(gitInfo.value?.available).toBe(false));
    expect(ask).not.toHaveBeenCalled();
  });
  it('turning it on with a ref compares, and off is only off', async () => {
    server({ '/api/git': () => json(info()), '/api/compare': () => json(changed()) });
    await loadGit();
    refName.value = 'main';
    setShowChanges(true);
    expect(showChanges.value).toBe(true);
    await vi.waitFor(() => expect(compare.value).not.toBeNull());
    setShowChanges(false);
    expect(showChanges.value).toBe(false);
    expect(compare.value).not.toBeNull();
  });
});

describe('what the repository says', () => {
  it('is kept, and a failure is said', async () => {
    server({ '/api/git': () => json(info()) });
    expect((await loadGit())?.available).toBe(true);
    expect(gitInfo.value?.branch).toBe('feature');
    server({ '/api/git': () => new Response('boom', { status: 500 }) });
    gitInfo.value = null;
    expect(await loadGit()).toBeNull();
  });
});

describe('the file at the ref', () => {
  it('is read once for a commit, and is null where the ref has no such file', async () => {
    const fetchMock = server({
      '/api/compare': () => json(changed()),
      '/api/git/file': (url) => (url.searchParams.get('path') === 'src/new.nsm' ? new Response('no', { status: 404 }) : json({ path: 'src/main.nsm', text: 'old', stamp: 'x', readonly: true })),
    });
    await runCompare('main');
    expect(await baseText('src/main.nsm')).toBe('old');
    expect(await baseText('src/main.nsm')).toBe('old');
    expect(await baseText('src/new.nsm')).toBeNull();
    expect(await baseText('src/new.nsm')).toBeNull();
    const asked = fetchMock.mock.calls.map((c) => String(c[0])).filter((u) => u.includes('/api/git/file'));
    expect(asked).toHaveLength(2);
    // by the commit's hash, not the name the person typed
    expect(asked[0]).toContain('ref=86d3736cc7b9f7660d36b29acc324b12ee90fd7b');
  });
  it('is null with no comparison', async () => {
    expect(await baseText('src/main.nsm')).toBeNull();
  });
});
