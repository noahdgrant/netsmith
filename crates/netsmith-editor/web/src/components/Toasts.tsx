// Toasts: what a finished action says, with its cause when it failed and a button to go on from
// there. A toast of success goes by itself; a failure stays until it is read.
//
// A toast goes out as it came in: the one that is dismissed fades and folds its height away, so the
// others do not jump. The stack stands above anything that is marked `data-toast-clear` (the zoom bar
// of a drawing, the mates of the system view, ...) and is on show, so a toast never covers a control.

import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';

import { type ToastItem, dismissToast, holdToast, releaseToast, toasts } from '../feedback/state';
import { CheckIcon, Icon, SeverityIcon } from './Icons';

/** How long a toast takes to leave (the same as the CSS: `.toast.leaving`). */
const LEAVE_MS = 160;

interface Shown {
  item: ToastItem;
  leaving: boolean;
}

/** How far up the stack must stand to clear what is marked `data-toast-clear` in the window's bottom right. */
function lift(stack: HTMLElement): number {
  const mine = stack.getBoundingClientRect();
  let need = 0;
  document.querySelectorAll<HTMLElement>('[data-toast-clear]').forEach((el) => {
    if (el.closest('.away')) return;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0 || getComputedStyle(el).visibility === 'hidden') return;
    // Only what is under the stack's columns matters.
    if (r.right < mine.left || r.left > mine.right) return;
    need = Math.max(need, window.innerHeight - r.top + 10);
  });
  return need;
}

export function Toasts() {
  const list = toasts.value;
  const [shown, setShown] = useState<Shown[]>(() => list.map((item) => ({ item, leaving: false })));
  const stack = useRef<HTMLDivElement>(null);
  const [raise, setRaise] = useState(0);

  // What is in the signal is shown; what left it stays a moment, leaving.
  useLayoutEffect(() => {
    const now = new Set(list.map((t) => t.id));
    const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    const gone = shown.filter((s) => !s.leaving && !now.has(s.item.id));
    for (const s of gone) {
      const el = stack.current?.querySelector<HTMLElement>(`[data-toast="${s.item.id}"]`);
      if (el) el.style.maxHeight = `${el.offsetHeight}px`;
    }
    setShown((prev) => {
      const kept = prev
        .filter((s) => s.leaving || now.has(s.item.id) || gone.includes(s))
        .map((s) => (now.has(s.item.id) ? { item: list.find((t) => t.id === s.item.id)!, leaving: false } : { ...s, leaving: true }));
      const have = new Set(kept.map((s) => s.item.id));
      return [...kept, ...list.filter((t) => !have.has(t.id)).map((item) => ({ item, leaving: false }))];
    });
    if (gone.length) {
      const ids = new Set(gone.map((s) => s.item.id));
      const t = setTimeout(() => setShown((prev) => prev.filter((s) => !(s.leaving && ids.has(s.item.id)))), reduced ? 0 : LEAVE_MS + 20);
      return () => clearTimeout(t);
    }
  }, [list]);

  // Clear the controls under the stack, now and when the window or what is marked changes size.
  useEffect(() => {
    const el = stack.current;
    if (!el) return;
    const measure = () => setRaise(lift(el));
    measure();
    const marked = [...document.querySelectorAll<HTMLElement>('[data-toast-clear]')];
    const observer = typeof ResizeObserver === 'function' ? new ResizeObserver(measure) : null;
    marked.forEach((m) => observer?.observe(m));
    window.addEventListener('resize', measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [shown.length]);

  return (
    <div class="toasts" ref={stack} style={raise ? { bottom: `calc(${raise}px)` } : undefined} aria-live="polite" aria-relevant="additions">
      {shown.map(({ item: t, leaving }) => (
        <div
          key={t.id}
          data-toast={t.id}
          class={`toast ${t.kind} ${leaving ? 'leaving' : ''}`}
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
          <button class="icon-btn" aria-label="Dismiss" data-tip="Dismiss" onClick={() => dismissToast(t.id)}>
            <Icon name="close" size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
