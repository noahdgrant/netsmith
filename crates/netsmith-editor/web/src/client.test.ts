import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiFailure, api, wasCancelled } from './client';

const reply = (status: number, body: string, type: string) => new Response(body, { status, headers: { 'content-type': type } });

afterEach(() => vi.unstubAllGlobals());

describe('a failed read', () => {
  it('asks for the structured form and shows the message of a failure of the project data, with its code', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      reply(422, JSON.stringify({ code: 'footprint_lacks_pad', message: 'U1: the footprint has no pad 101', part: 'U1' }), 'application/json'),
    );
    vi.stubGlobal('fetch', fetchMock);
    const failure = await api.pcbScene('b').catch((e: unknown) => e);
    expect(fetchMock.mock.calls[0][1].headers.Accept).toBe('application/json');
    expect(failure).toBeInstanceOf(ApiFailure);
    expect((failure as ApiFailure).status).toBe(422);
    expect((failure as ApiFailure).code).toBe('footprint_lacks_pad');
    expect((failure as ApiFailure).message).toBe('U1: the footprint has no pad 101');
  });

  it('keeps the plain text of any other failure, with no code', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(reply(500, 'boom\n', 'text/plain')));
    const failure = (await api.pcbScene('b').catch((e: unknown) => e)) as ApiFailure;
    expect(failure.status).toBe(500);
    expect(failure.code).toBeNull();
    expect(failure.message).toBe('boom');
  });

  it('keeps the text of a JSON body that is not a structured failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(reply(500, '{"x":1}', 'application/json')));
    const failure = (await api.pcbScene('b').catch((e: unknown) => e)) as ApiFailure;
    expect(failure.message).toBe('{"x":1}');
    expect(failure.code).toBeNull();
  });
});

describe('cancelling', () => {
  it('posts to the route of the job, with the guard header, and reads whether one was running', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => reply(200, '{"cancelled":true}', 'application/json'));
    vi.stubGlobal('fetch', fetchMock);
    for (const [call, route] of [
      [() => api.cancelAnalysis(), '/api/analysis/cancel'],
      [() => api.cancelCompare(), '/api/compare/cancel'],
      [() => api.cancelBuild(), '/api/build/cancel'],
      [() => api.cancelFab(), '/api/fab/cancel'],
      [() => api.cancelOrder(), '/api/order/cancel'],
    ] as const) {
      fetchMock.mockClear();
      expect(await call()).toEqual({ cancelled: true });
      expect(fetchMock.mock.calls[0][0]).toBe(route);
      expect(fetchMock.mock.calls[0][1].method).toBe('POST');
      expect(fetchMock.mock.calls[0][1].headers['X-Netsmith']).toBe('1');
    }
  });

  it('asks to analyse again with a POST', async () => {
    const fetchMock = vi.fn().mockResolvedValue(reply(200, '{"version":3}', 'application/json'));
    vi.stubGlobal('fetch', fetchMock);
    await api.runAnalysis();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/analysis/run');
    expect(fetchMock.mock.calls[0][1].method).toBe('POST');
  });

  it('knows a cancelled job by the 499 it answers with, and takes nothing else for one', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(reply(499, 'the compare was cancelled\n', 'text/plain')));
    const failure = await api.compare('main').catch((e: unknown) => e);
    expect(wasCancelled(failure)).toBe(true);
    expect((failure as ApiFailure).message).toBe('the compare was cancelled');
    expect(wasCancelled(new ApiFailure(500, 'boom'))).toBe(false);
    expect(wasCancelled(new Error('x'))).toBe(false);
    expect(wasCancelled(null)).toBe(false);
  });
});

describe('the schematic layout file', () => {
  it('is read with a GET of the board and written with a PUT of its text that has the guard header', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => reply(200, '{"board":"Led/main","exists":false}', 'application/json'));
    vi.stubGlobal('fetch', fetchMock);
    await api.schematicHints('Led/main');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/schematic-hints?board=Led%2Fmain');
    expect(fetchMock.mock.calls[0][1].headers.Accept).toBe('application/json');

    fetchMock.mockClear();
    await api.writeSchematicHints('Led/main', 'version = 1\n', '12-34');
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/schematic-hints?board=Led%2Fmain');
    expect(init.method).toBe('PUT');
    expect(init.body).toBe('version = 1\n');
    expect(init.headers['X-Netsmith']).toBe('1');
    expect(init.headers['X-Stamp']).toBe('12-34');

    fetchMock.mockClear();
    await api.writeSchematicHints('Led/main', 'version = 1\n');
    expect('X-Stamp' in fetchMock.mock.calls[0][1].headers).toBe(false);
  });

  it('says a text that does not read with the code and the message, and a stale stamp with 409', async () => {
    const body = { code: 'schematic_hints_invalid', message: 'src/main.sch.toml:3: expected a key', file: 'src/main.sch.toml', line: '3' };
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(reply(422, JSON.stringify(body), 'application/json')));
    const failure = (await api.writeSchematicHints('b', 'x').catch((e: unknown) => e)) as ApiFailure;
    expect(failure).toBeInstanceOf(ApiFailure);
    expect([failure.status, failure.code]).toEqual([422, 'schematic_hints_invalid']);
    expect(failure.message).toBe('src/main.sch.toml:3: expected a key');

    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(reply(409, 'changed on disk\n', 'text/plain')));
    expect(((await api.writeSchematicHints('b', 'x', '1').catch((e: unknown) => e)) as ApiFailure).status).toBe(409);
  });
});
