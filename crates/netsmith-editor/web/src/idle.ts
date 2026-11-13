// What the page says of itself to a program that drives it (the browser recipes of
// scripts/editor-verify): `<html data-busy>` is there while the page has work in flight (a request, a
// pause before an action, an analysis, a transition), and `window.__netsmith.idle()` resolves once it
// has had none for three frames. A recipe waits on that, never for a time.

import { effect, type ReadonlySignal } from '@preact/signals';

let holds = 0;
const sources: ReadonlySignal<boolean>[] = [];

/** A thing the page waits for has begun: call what this returns when it ends (twice is the same as once). */
export function hold(): () => void {
  holds++;
  mark();
  let released = false;
  return () => {
    if (released) return;
    released = true;
    holds--;
    mark();
  };
}

/** Run `work`, and stay busy until what it returns is settled. */
export async function held<T>(work: () => Promise<T>): Promise<T> {
  const release = hold();
  try {
    return await work();
  } finally {
    release();
  }
}

/** `setTimeout` for a pause the page makes before it acts (a debounce, a hover's rest): busy until it has run or is cancelled. Returns the cancel. */
export function after(ms: number, run: () => void): () => void {
  const release = hold();
  const timer = setTimeout(() => {
    release();
    run();
  }, ms);
  return () => {
    clearTimeout(timer);
    release();
  };
}

/** Busy while `source` is true (the analysis, for one). */
export function busyWhile(source: ReadonlySignal<boolean>) {
  sources.push(source);
  effect(() => {
    void source.value;
    mark();
  });
}

const probes = new Set<() => boolean>();

/** Something the page can only ask (the 3D view's camera is moving): `idle()` waits while `moving()` is true. Returns what stops the asking. */
export function watchMoving(moving: () => boolean): () => void {
  probes.add(moving);
  return () => void probes.delete(moving);
}

/** A transition or an animation that ends: not a spinner that never does. */
function moving(): boolean {
  return document.getAnimations().some((a) => {
    if (a.playState !== 'running' && !a.pending) return false;
    return Number.isFinite(a.effect?.getComputedTiming().endTime ?? 0);
  });
}

const working = () => holds > 0 || sources.some((s) => s.peek());

function mark() {
  // The page's unit tests run with no document.
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  if (working()) root.dataset.busy = '1';
  else delete root.dataset.busy;
}

/** Resolves when nothing is in flight and nothing moves, for three frames in a row. */
function idle(): Promise<void> {
  return new Promise((resolve) => {
    let quiet = 0;
    const frame = () => {
      if (working() || moving() || [...probes].some((p) => p())) quiet = 0;
      else if (++quiet >= 3) return resolve();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  });
}

declare global {
  interface Window {
    __netsmith?: { idle: () => Promise<void> };
  }
}

export function installIdle() {
  window.__netsmith = { idle };
  mark();
}
