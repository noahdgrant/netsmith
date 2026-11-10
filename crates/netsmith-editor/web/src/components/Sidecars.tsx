// The two files a run of netsmith keeps beside a source file, shown read only in the inspector's
// own pattern: the pin map of the selected part, and the board's pick lock. Neither writes.

import type { PartInfo, PickLockInfo } from '../api';
import { lockView, pinRows, withoutPin } from '../sidecars';
import { Defs, InspectorSection } from './Inspector';
import { Icon } from './Icons';

/** The pin requests of a part with the pin each got; nothing for a part no request asks. */
export function PinMap({ part }: { part: PartInfo }) {
  const rows = pinRows(part.pinRequests);
  if (rows.length === 0) return null;
  const missing = withoutPin(rows);
  return (
    <InspectorSection title="Pin map" count={missing > 0 ? `${rows.length}, ${missing} without a pin` : rows.length}>
      <ul class="pinmap" aria-label="Pin requests" data-testid="pin-map">
        {rows.map((r) => (
          <li class={`pm-row ${r.message ? 'none' : ''}`} key={r.request} data-request={r.request}>
            <span class="pm-req mono" title="The request, as the pin file keys it">
              {r.request}
            </span>
            {r.message ? (
              <span class="pm-pin faint">no pin</span>
            ) : (
              <span class="pm-pin mono" title="The pin in the part, and its pad in the package">
                <span class="pm-name">{r.pin}</span>
                {r.pad && <span class="faint"> · {r.pad}</span>}
              </span>
            )}
            <span class="pm-meta">
              <span class="mono pm-fn" title="The function the request asks for">
                {r.function}
              </span>
              {r.locked && (
                <span class="tag lock pm-lock" title="Locked: a run never moves this pin">
                  <Icon name="lock" size={11} /> locked
                </span>
              )}
              {r.origin && (
                <span class="faint pm-origin" title={r.originTitle}>
                  {r.origin}
                </span>
              )}
            </span>
            {r.message && <span class="pm-msg">{r.message}</span>}
          </li>
        ))}
      </ul>
    </InspectorSection>
  );
}

/** The board's pick lock: whether the file exists, whether it is current, and the diff `netsmith lock --check` prints. It is open
 * when there is something to read (a lock that is not current, absent or unreadable) and folded when it is fine; the state is in
 * its header either way, and a change of state opens or folds it again. */
export function PickLock({ lock }: { lock: PickLockInfo }) {
  const v = lockView(lock);
  return (
    <InspectorSection
      key={v.tone}
      title="Pick lock"
      open={v.tone !== 'ok'}
      count={
        <span class={`lock-state ${v.tone}`} data-testid="pick-lock-state" data-state={v.state}>
          {v.state}
        </span>
      }
    >
      <div data-testid="pick-lock">
        <Defs
          rows={[
            { label: 'File', value: lock.file, mono: true, title: 'Beside the source file, committed with it', tone: v.tone === 'ok' ? undefined : v.tone },
          ]}
        />
        <p class="ih-help lock-says">{v.says}</p>
        {v.lines.length > 0 && (
          <ul class="lock-diff mono" aria-label="What would change in the lock">
            {v.lines.map((l) => (
              <li key={l}>{l}</li>
            ))}
          </ul>
        )}
        {lock.reported && (
          <details class="lock-reported">
            <summary>What the solver said of the rows it holds</summary>
            <pre>{lock.reported}</pre>
          </details>
        )}
      </div>
    </InspectorSection>
  );
}
