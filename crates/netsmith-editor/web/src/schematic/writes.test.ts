import { describe, expect, it } from 'vitest';

import { OneAtATime } from './writes';

/** A job that ends when `finish` is called. */
function gate() {
  let finish!: () => void;
  const done = new Promise<void>((r) => (finish = r));
  return { done, finish };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

describe('the writes of the layout file', () => {
  it('start at once when none is on its way', async () => {
    const ran: string[] = [];
    const q = new OneAtATime<string>(async (j) => void ran.push(j));
    expect(q.submit('a')).toBe('started');
    await tick();
    expect(ran).toEqual(['a']);
    expect(q.busy).toBe(false);
  });

  it('never run two at once: a second waits, and runs after the first', async () => {
    const order: string[] = [];
    const g = gate();
    const q = new OneAtATime<string>(async (j) => {
      order.push(`begin ${j}`);
      if (j === 'a') await g.done;
      order.push(`end ${j}`);
    });
    expect(q.submit('a')).toBe('started');
    expect(q.submit('b')).toBe('queued');
    expect(q.busy && q.queued).toBe(true);
    await tick();
    expect(order).toEqual(['begin a']);
    g.finish();
    await tick();
    await tick();
    expect(order).toEqual(['begin a', 'end a', 'begin b', 'end b']);
    expect(q.busy).toBe(false);
    expect(q.queued).toBe(false);
  });

  it('keeps exactly one waiting: a third drop is refused', async () => {
    const g = gate();
    const ran: string[] = [];
    const q = new OneAtATime<string>(async (j) => {
      ran.push(j);
      if (j === 'a') await g.done;
    });
    q.submit('a');
    expect(q.submit('b')).toBe('queued');
    expect(q.submit('c')).toBe('refused');
    g.finish();
    await tick();
    await tick();
    expect(ran).toEqual(['a', 'b']);
  });

  it('goes on to the waiting job when the first fails, and takes new jobs after', async () => {
    const ran: string[] = [];
    const g = gate();
    const q = new OneAtATime<string>(async (j) => {
      ran.push(j);
      if (j === 'a') {
        await g.done;
        throw new Error('no');
      }
    });
    q.submit('a');
    q.submit('b');
    g.finish();
    await tick();
    await tick();
    expect(ran).toEqual(['a', 'b']);
    expect(q.submit('c')).toBe('started');
  });

  it('says when busy or queued changes, so the page shows the redrawing', async () => {
    const states: string[] = [];
    const g = gate();
    const q = new OneAtATime<string>(
      async () => g.done,
      () => states.push(`${q.busy ? 'busy' : 'idle'}${q.queued ? '+queued' : ''}`),
    );
    q.submit('a');
    q.submit('b');
    g.finish();
    await tick();
    await tick();
    expect(states[0]).toBe('busy');
    expect(states).toContain('busy+queued');
    expect(states[states.length - 1]).toBe('idle');
  });
});
