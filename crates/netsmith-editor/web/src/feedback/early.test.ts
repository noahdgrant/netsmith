import { describe, expect, it } from 'vitest';

import { RETRY_FIRST_MS, RETRY_MAX_MS, nextDelay, pendingElapsed } from './early';

describe('pendingElapsed', () => {
  it("reads the server's pending answer", () => {
    expect(pendingElapsed(503, '{"pending":true,"elapsedMs":12345}')).toBe(12345);
    expect(pendingElapsed(503, '{"pending":true}')).toBe(0);
  });
  it('leaves every other answer alone', () => {
    expect(pendingElapsed(200, '{"pending":true,"elapsedMs":1}')).toBeNull();
    // A 503 for a page that is not there, as text, or JSON that is not ours.
    expect(pendingElapsed(503, 'the page is not built')).toBeNull();
    expect(pendingElapsed(503, '{"error":"no"}')).toBeNull();
    expect(pendingElapsed(503, '{"pending":"yes"}')).toBeNull();
    expect(pendingElapsed(503, 'null')).toBeNull();
  });
});

describe('nextDelay', () => {
  it('starts at a second and doubles up to the cap', () => {
    const waits = [0, 0, 0, 0, 0].reduce<number[]>((all) => [...all, nextDelay(all[all.length - 1] ?? 0)], []);
    expect(waits).toEqual([RETRY_FIRST_MS, 2000, 4000, RETRY_MAX_MS, RETRY_MAX_MS]);
  });
});
