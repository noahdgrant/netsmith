import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { after, hold, held, installIdle, watchMoving } from './idle';

// The page is idle when nothing is in flight and nothing moves for three frames: a recipe waits on this instead of for a time.
describe('idle', () => {
  const root = { dataset: {} as Record<string, string> };
  let frames: (() => void)[] = [];
  const frame = () => frames.splice(0).forEach((f) => f());

  beforeEach(() => {
    root.dataset = {};
    frames = [];
    vi.useFakeTimers();
    vi.stubGlobal('document', { documentElement: root, getAnimations: () => [] });
    vi.stubGlobal('window', {});
    vi.stubGlobal('requestAnimationFrame', (f: () => void) => frames.push(f));
    installIdle();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  const idle = () => (window as unknown as { __netsmith: { idle: () => Promise<void> } }).__netsmith.idle();

  /** Whether `idle()` has resolved after `n` frames. */
  async function resolvedAfter(n: number): Promise<boolean> {
    let done = false;
    void idle().then(() => (done = true));
    for (let i = 0; i < n; i++) {
      frame();
      await Promise.resolve();
    }
    return done;
  }

  it('resolves after three quiet frames', async () => {
    expect(await resolvedAfter(2)).toBe(false);
    expect(await resolvedAfter(5)).toBe(true);
  });

  it('is busy while something is held, and says so in data-busy', async () => {
    const release = hold();
    expect(root.dataset.busy).toBe('1');
    expect(await resolvedAfter(10)).toBe(false);
    release();
    release();
    expect(root.dataset.busy).toBeUndefined();
    expect(await resolvedAfter(5)).toBe(true);
  });

  it('is busy until a pause has run, or is cancelled', async () => {
    const ran = vi.fn();
    after(100, ran);
    expect(root.dataset.busy).toBe('1');
    vi.advanceTimersByTime(100);
    expect(ran).toHaveBeenCalledOnce();
    expect(root.dataset.busy).toBeUndefined();
    const cancel = after(100, ran);
    cancel();
    vi.advanceTimersByTime(200);
    expect(ran).toHaveBeenCalledOnce();
    expect(root.dataset.busy).toBeUndefined();
  });

  it('is busy while a request is under way, also when it fails', async () => {
    let fail: (e: Error) => void = () => {};
    const request = held(() => new Promise<void>((_, reject) => (fail = reject)));
    expect(root.dataset.busy).toBe('1');
    fail(new Error('no'));
    await expect(request).rejects.toThrow('no');
    expect(root.dataset.busy).toBeUndefined();
  });

  it('waits while something the page can only ask about is moving', async () => {
    let moving = true;
    const stop = watchMoving(() => moving);
    expect(await resolvedAfter(10)).toBe(false);
    moving = false;
    expect(await resolvedAfter(5)).toBe(true);
    stop();
  });
});
