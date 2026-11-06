import { useEffect, useState } from 'preact/hooks';

import { api } from '../client';
import { job, say } from '../store';
import { seconds } from '../util';

/** The jobs the Cancel button stops: the schematic build, a fab and an order. (The router has its own Stop on the PCB view,
 * which keeps what it routed.) */
const CANCEL: Record<string, () => Promise<{ cancelled: boolean }>> = {
  build: () => api.cancelBuild(),
  fab: () => api.cancelFab(),
  order: () => api.cancelOrder(),
};

/** Ask the server to stop the running job of `task`: it ends at its next checkpoint and its request is answered `cancelled`. */
export async function cancelJob(task: string): Promise<void> {
  const cancel = CANCEL[task];
  if (!cancel) return;
  try {
    await cancel();
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
  }
}

/** Time since `since`, ticking while it is shown. */
export function useElapsed(since: number | null): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (since == null) return;
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(timer);
  }, [since]);
  return since == null ? 0 : Math.max(0, now - since);
}

/** What the running job has said, with a spinner and the time it has taken. Nothing when no
 * job of `task` runs. */
export function JobProgress({ task }: { task: string }) {
  const j = job.value;
  const elapsed = useElapsed(j?.task === task ? j.started : null);
  if (!j || j.task !== task) return null;
  const last = j.lines[j.lines.length - 1];
  return (
    <div class="job">
      <div class="job-head">
        <span class="spinner" />
        <span class="job-line">{last ?? 'starting'}</span>
        <span class="job-time">{seconds(elapsed)}</span>
        {task in CANCEL && (
          <button class="tool small" data-testid={`cancel-${task}`} data-tip="Stop this. Nothing of what it is making is kept" onClick={() => void cancelJob(task)}>
            Cancel
          </button>
        )}
      </div>
      {j.lines.length > 1 && (
        <ul class="job-lines">
          {j.lines.slice(-6, -1).map((l, i) => (
            <li key={i}>{l}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
