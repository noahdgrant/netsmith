// Toasts: what a finished action says, with its cause when it failed and a button to go on from
// there. A toast of success goes by itself; a failure stays until it is read.

import { dismissToast, holdToast, releaseToast, toasts } from '../feedback/state';
import { CheckIcon, SeverityIcon } from './ProblemsDock';

export function Toasts() {
  const list = toasts.value;
  return (
    <div class="toasts" aria-live="polite" aria-relevant="additions">
      {list.map((t) => (
        <div
          key={t.id}
          class={`toast ${t.kind}`}
          role={t.kind === 'bad' ? 'alert' : 'status'}
          onPointerEnter={() => holdToast(t.id)}
          onPointerLeave={() => releaseToast(t.id)}
        >
          <span class="toast-icon">{t.kind === 'bad' ? <SeverityIcon severity="error" size={16} /> : t.kind === 'ok' ? <CheckIcon size={16} /> : <SeverityIcon severity="note" size={16} />}</span>
          <div class="toast-body">
            <div class="toast-title">{t.title}</div>
            {t.detail && <div class="toast-detail">{t.detail}</div>}
            {t.actions.length > 0 && (
              <div class="toast-actions">
                {t.actions.map((a) => (
                  <button
                    key={a.label}
                    class="tool small"
                    onClick={() => {
                      dismissToast(t.id);
                      a.run();
                    }}
                  >
                    {a.label}
                  </button>
                ))}
              </div>
            )}
          </div>
          <button class="icon-btn" aria-label="Dismiss" title="Dismiss" onClick={() => dismissToast(t.id)}>
            <svg class="icon" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
              <path d="M4 4l8 8M12 4l-8 8" />
            </svg>
          </button>
        </div>
      ))}
    </div>
  );
}
