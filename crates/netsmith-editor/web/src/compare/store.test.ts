import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { board } from '../store';
import { chooseRef, clearCompare, compare, changes, baseText, gitInfo, loadGit, marks, refName, runCompare, scope, scopeChoice, setShowChanges, showChanges, state } from './store';
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
  it('is the board in front when the comparison has it, else the one board, else all of them', () => {
    board.value = 'TwoBoards/led';
    expect(scope.value).toBe('TwoBoards/led');
    board.value = 'Other/board';
    expect(scope.value).toBe('all');
    board.value = null;
    expect(scope.value).toBe('all');
  });
  it('is what was chosen, when the comparison has it', () => {
    board.value = 'TwoBoards/led';
    scopeChoice.value = 'TwoBoards/power';
    expect(scope.value).toBe('TwoBoards/power');
    scopeChoice.value = 'all';
    expect(scope.value).toBe('all');
    scopeChoice.value = 'Gone/board';
    expect(scope.value).toBe('TwoBoards/led');
  });
});

describe('the switch', () => {
  it('turning it on with no ref asks for one, and leaves it off to be turned on by the answer', () => {
    const ask = vi.fn();
    setShowChanges(true, ask);
    expect(ask).toHaveBeenCalledOnce();
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
