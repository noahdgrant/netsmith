// A command's output as a panel: monospace, a line number for each line, a colour for the
// severity of the line (error, warning, note, ok), folded or open, and copyable.

import type { ComponentChildren } from 'preact';
import { useState } from 'preact/hooks';

import { Icon } from './Icons';
import { CopyButton } from './ui';

export type LineSeverity = 'bad' | 'warn' | 'info' | 'ok' | '';

/** What a line of netsmith's output says about itself. Lines that say nothing in particular stay plain. */
export function lineSeverity(line: string): LineSeverity {
  if (/^\s*(error|fatal)\b|\berror\[|:\s*error\b|\bFAILED\b|\bstopped\b|\bcannot\b|\bviolations?\b/i.test(line) && !/\b0 errors?\b/i.test(line)) return 'bad';
  if (/^\s*warning\b|:\s*warning\b|\[constraint\]|\bunverified\b|\bnot checked\b/i.test(line)) return 'warn';
  if (/^\s*(note|info)\b|:\s*note\b/i.test(line)) return 'info';
  if (/\bpassed\b|^\s*wrote\b|\bmade\b|\bwritten\b/i.test(line)) return 'ok';
  return '';
}

export function LogPanel({
  title,
  note,
  text,
  open: initial = true,
  tone,
  class: cls,
  actions,
  max,
}: {
  title: string;
  note?: ComponentChildren;
  text: string;
  open?: boolean;
  /** `bad`: the whole panel is an error, not only some lines. */
  tone?: 'bad';
  class?: string;
  actions?: ComponentChildren;
  /** The most lines shown before it scrolls (the panel's height). */
  max?: number;
}) {
  const [open, setOpen] = useState(initial);
  const lines = text.replace(/\n+$/, '').split('\n');
  return (
    <div class={`log ${tone ?? ''} ${open ? 'open' : ''} ${cls ?? ''}`}>
      <div class="log-head">
        <button type="button" class="log-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          <Icon name="chevron-right" size={12} class="log-chev" />
          <span class="log-title">{title}</span>
          <span class="log-count">{lines.length} {lines.length === 1 ? 'line' : 'lines'}</span>
          {note && <span class="log-note">{note}</span>}
        </button>
        <span class="log-actions">
          {actions}
          <CopyButton text={text} label="Copy the output" />
        </span>
      </div>
      {open && (
        <pre class="log-body" style={max ? { maxHeight: `${max * 18 + 16}px` } : undefined} tabIndex={0}>
          {lines.map((l, i) => (
            <span key={i} class={`ln ${tone === 'bad' ? 'bad' : lineSeverity(l)}`}>
              {l || ' '}
            </span>
          ))}
        </pre>
      )}
    </div>
  );
}
