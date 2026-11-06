import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiFailure, api } from './client';

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
