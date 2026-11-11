// The two files a run of netsmith keeps beside a source file, in the inspector's own pattern: the pin map of the selected part,
// which writes the pin file's `lock`, `pin` and `source` lines (a padlock, a move), and the board's pick lock, which is read only.

import { useEffect, useRef } from 'preact/hooks';

import type { PartInfo, PickLockInfo } from '../api';
import { dragging, moving, movePin, pinBusy, setLock } from '../pinwrites';
import { type PinRow, cannotMove, lockView, padlockTitle, pinFileOf, pinRows, withoutPin } from '../sidecars';
import { detail } from '../store';
import { Defs, InspectorSection } from './Inspector';
import { Icon } from './Icons';

/** The pin requests of a part with the pin each got; nothing for a part no request asks. Each has a padlock (the pin file's `lock`
 * line) and a move (its `pin` and `source` lines): a list of the pins that offer the function, which a pin dragged by the pointer
 * is dropped on, and which the keyboard walks. */
export function PinMap({ part }: { part: PartInfo }) {
  const rows = pinRows(part.pinRequests);
  if (rows.length === 0) return null;
  const missing = withoutPin(rows);
  const file = pinFileOf(detail.value?.file ?? 'main.nsm');
  return (
    <InspectorSection title="Pin map" count={missing > 0 ? `${rows.length}, ${missing} without a pin` : rows.length}>
      <ul class="pinmap" aria-label="Pin requests" data-testid="pin-map">
        {rows.map((r) => (
          <PinRowView key={r.request} r={r} file={file} />
        ))}
      </ul>
    </InspectorSection>
  );
}

function PinRowView({ r, file }: { r: PinRow; file: string }) {
  const busy = pinBusy.value;
  const open = moving.value === r.request;
  const blocked = cannotMove(r);
  const rowRef = useRef<HTMLLIElement>(null);
  const moveRef = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(false);
  // A list that closes (the move was written, or Escape) must not leave the keyboard on a button that is gone.
  useEffect(() => {
    const active = document.activeElement;
    if (wasOpen.current && !open && (active === document.body || rowRef.current?.contains(active))) moveRef.current?.focus();
    wasOpen.current = open;
  }, [open]);
  const drag = !blocked && !busy;
  return (
    <li class={`pm-row ${r.message ? 'none' : ''}`} ref={rowRef} data-request={r.request}>
      <span class="pm-req mono" title="The request, as the pin file keys it">
        {r.request}
      </span>
      {r.message ? (
        <span class="pm-pin faint">no pin</span>
      ) : (
        <span class="pm-pinbox">
          <span
            class={`pm-pin mono ${drag ? 'grab' : ''}`}
            draggable={drag}
            title={drag ? 'The pin in the part, and its pad in the package. Drag it onto another pin that offers the function to move the request.' : 'The pin in the part, and its pad in the package'}
            onDragStart={(e) => {
              e.dataTransfer?.setData('text/plain', r.request);
              if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
              dragging.value = r.request;
              moving.value = r.request;
            }}
            onDragEnd={() => {
              dragging.value = null;
            }}
            data-testid="pm-pin"
          >
            <span class="pm-name">{r.pin}</span>
            {r.pad && <span class="faint"> · {r.pad}</span>}
          </span>
          <button
            type="button"
            class={`pm-padlock ${r.locked ? 'on' : ''}`}
            aria-pressed={r.locked}
            aria-label={`Lock the pin of ${r.request}`}
            title={padlockTitle(r, file)}
            disabled={busy}
            onClick={() => void setLock(r.request, !r.locked)}
            data-testid="pm-padlock"
          >
            <Icon name={r.locked ? 'lock' : 'unlock'} size={13} />
          </button>
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
        {!r.message && (
          <button
            type="button"
            class="link pm-move"
            ref={moveRef}
            aria-expanded={open}
            aria-label={`Move ${r.request} to another pin`}
            aria-disabled={blocked !== null || busy}
            title={blocked ?? `Choose another pin that offers ${r.function}: writes \`pin\` and \`source = "hand"\` in ${file}`}
            onClick={() => {
              if (blocked !== null || busy) return;
              moving.value = open ? null : r.request;
            }}
            data-testid="pm-move"
          >
            Move
          </button>
        )}
      </span>
      {r.message && <span class="pm-msg">{r.message}</span>}
      {open && !r.message && (
        <div
          class="pm-moves"
          role="group"
          aria-label={`Pins ${r.request} may move to`}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.stopPropagation();
              moving.value = null;
            }
          }}
        >
          <p class="ih-help pm-moves-help">
            Choose a pin, or drag the pin of <span class="mono">{r.request}</span> onto one. Free pins are outlined; a pin another request holds is refused.
          </p>
          <ul class="pm-targets" aria-label={`Pins that offer ${r.function}`}>
            {r.targets.map((t) => (
              <li key={t.pin}>
                <button
                  type="button"
                  class={`pm-target ${t.state}`}
                  title={t.title}
                  aria-label={t.label}
                  aria-current={t.state === 'current' ? 'true' : undefined}
                  aria-disabled={t.state === 'current' || busy}
                  data-pin={t.pin}
                  data-state={t.state}
                  onClick={() => {
                    if (t.state !== 'current' && !busy) void movePin(r.request, t.pin);
                  }}
                  onDragOver={(e) => {
                    if (dragging.value === r.request && t.state !== 'current') {
                      e.preventDefault();
                      if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
                    }
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    const from = dragging.value;
                    dragging.value = null;
                    if (from === r.request && t.state !== 'current' && !busy) void movePin(r.request, t.pin);
                  }}
                >
                  <span class="mono">{t.pin}</span>
                  {t.pad && <span class="faint"> · {t.pad}</span>}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </li>
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
