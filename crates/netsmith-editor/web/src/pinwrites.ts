// The padlock and the move of the pin map, which write the pin file's `lock`, `pin` and `source` lines (ADR 0101). One write at a
// time: each makes the server analyse the project again, and the page reads that analysis before the next click counts. A refusal
// (the server says why: a pin held by another request, a pin that does not offer the function, a locked request) is a toast with
// the server's words and writes nothing.

import { signal } from '@preact/signals';

import type { PinWritten } from './api';
import { ApiFailure, api, wasCancelled } from './client';
import { notify } from './feedback/state';
import { awaitRebuild, detail, say } from './store';

/** Whether a write is on its way: the padlocks and the move lists wait. */
export const pinBusy = signal(false);

/** The request whose pin is being dragged, so that only its own move list takes the drop. */
export const dragging = signal<string | null>(null);

/** The request whose move list is open (at most one), by its key. */
export const moving = signal<string | null>(null);

async function write(request: string, what: string | null, run: (board: string) => Promise<PinWritten>): Promise<boolean> {
  const board = detail.peek()?.name;
  if (!board || pinBusy.peek()) return false;
  pinBusy.value = true;
  try {
    const written = await run(board);
    await awaitRebuild();
    if (what) say(`${what} (written to ${written.file})`);
    return true;
  } catch (e) {
    if (wasCancelled(e)) {
      say(`${what ?? 'The pin file is written'}. The analysis was cancelled; Run again reads the pin file.`);
      return true;
    }
    const message = e instanceof Error ? e.message : String(e);
    notify({
      kind: 'bad',
      title: `\`${request}\`: the pin file was not written`,
      detail: e instanceof ApiFailure && e.status === 409 ? `${message}.` : message,
    });
    // A refusal that follows a change on disk (409) is answered by the new reading.
    if (e instanceof ApiFailure && e.status === 409) await awaitRebuild();
    return false;
  } finally {
    pinBusy.value = false;
  }
}

/** Lock or unlock the pin of a request. */
export function setLock(request: string, locked: boolean): Promise<boolean> {
  // No toast: the padlock and the row's mark change, which says it.
  return write(request, null, (board) => api.pinLock({ board, request, locked }));
}

/** Put a request on another pin. The move list closes when it is written. */
export async function movePin(request: string, pin: string): Promise<boolean> {
  const done = await write(request, `Moved \`${request}\` to ${pin}`, (board) => api.pinMove({ board, request, pin }));
  if (done && moving.peek() === request) moving.value = null;
  return done;
}
