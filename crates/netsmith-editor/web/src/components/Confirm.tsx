// A question the page asks before it does something that cannot be undone from the page: a title, what will happen, and
// two buttons. Cancel has the focus, so Enter does not do it. It uses the file tree's dialog (`ConfirmRemove` looks the same).

import { signal } from '@preact/signals';
import { useEffect, useRef } from 'preact/hooks';

export interface Question {
  title: string;
  lines: string[];
  /** The label of the button that does it. */
  yes: string;
  run: () => void;
}

/** The question on screen, if any. */
export const question = signal<Question | null>(null);

export function ask(q: Question) {
  question.value = q;
}

export function Confirm() {
  const q = question.value;
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (q) cancel.current?.focus();
  }, [q]);
  if (!q) return null;
  const close = () => (question.value = null);
  return (
    <div class="ft-scrim" data-testid="confirm" onPointerDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="ft-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-text"
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Escape') {
            e.preventDefault();
            close();
          }
        }}
      >
        <h2 id="confirm-title">{q.title}</h2>
        <div id="confirm-text">
          {q.lines.map((l, i) => (
            <p key={i}>{l}</p>
          ))}
        </div>
        <div class="ft-dialog-buttons">
          <button type="button" ref={cancel} class="tool" onClick={close}>
            Cancel
          </button>
          <button
            type="button"
            class="tool danger"
            data-testid="confirm-yes"
            onClick={() => {
              close();
              q.run();
            }}
          >
            {q.yes}
          </button>
        </div>
      </div>
    </div>
  );
}
