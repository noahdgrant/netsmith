import { useEffect, useState } from 'preact/hooks';

import { job } from '../store';
import { seconds } from '../util';

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
