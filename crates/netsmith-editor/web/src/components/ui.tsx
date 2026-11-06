// Small parts the views share (round 2): a pill that carries a status, a copyable command, a
// popover, a segmented control and a spinner. Each is a few lines; the look is in `views.css`.

import type { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';

import { spaceUnits } from '../util';
import { Icon, type IconName } from './Icons';

export type Tone = 'neutral' | 'ok' | 'warn' | 'bad' | 'info' | 'sel';

/** A status as a pill: a dot or an icon, a word. The tone is the meaning; the colour follows the theme. */
export function Pill({
  tone = 'neutral',
  icon,
  dot,
  children,
  title,
  class: cls,
}: {
  tone?: Tone;
  icon?: IconName;
  dot?: boolean;
  children?: ComponentChildren;
  title?: string;
  class?: string;
}) {
  return (
    <span class={`chip pl pl-${tone} ${cls ?? ''}`} title={title}>
      {icon && <Icon name={icon} size={12} />}
      {dot && <i class="pl-dot" aria-hidden="true" />}
      {children}
    </span>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return <span class="spin" role="img" aria-label="Working" style={{ width: size, height: size }} />;
}

/** Copy text to the clipboard: the page's API where it has one, else the old way. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const area = document.createElement('textarea');
      area.value = text;
      area.setAttribute('readonly', '');
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.appendChild(area);
      area.select();
      const ok = document.execCommand('copy');
      area.remove();
      return ok;
    } catch {
      return false;
    }
  }
}

/** A button that copies `text` and says so for a moment. */
export function CopyButton({ text, label = 'Copy', class: cls }: { text: string; label?: string; class?: string }) {
  const [done, setDone] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <button
      type="button"
      class={`icon-btn copy-btn ${done ? 'done' : ''} ${cls ?? ''}`}
      data-tip={done ? 'Copied' : label}
      aria-label={done ? 'Copied' : label}
      onClick={async (e) => {
        e.stopPropagation();
        if (await copyText(text)) {
          setDone(true);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setDone(false), 1400);
        }
      }}
    >
      <Icon name={done ? 'check' : 'copy'} size={14} />
    </button>
  );
}

/** The command line that does the same: a code chip with a copy button. */
export function CliChip({ command }: { command: string }) {
  return (
    <span class="cli" data-testid="cli-chip">
      <Icon name="terminal" size={13} class="cli-icon" />
      <code>{command}</code>
      <CopyButton text={command} label="Copy the command" />
    </span>
  );
}

/** A panel that opens under its button and closes on Escape, a click outside or the button again. */
export function Popover({
  label,
  icon,
  tip,
  align = 'left',
  children,
  class: cls,
  badge,
  fixed,
}: {
  label?: string;
  icon?: IconName;
  tip: string;
  align?: 'left' | 'right';
  children: ComponentChildren | ((close: () => void) => ComponentChildren);
  class?: string;
  badge?: ComponentChildren;
  /** Placed in the window, not under the button: for a button inside a box that clips (the top bar). */
  fixed?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState<{ top: number; left?: number; right?: number } | null>(null);
  const root = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener('pointerdown', away, true);
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('pointerdown', away, true);
      document.removeEventListener('keydown', key, true);
    };
  }, [open]);
  return (
    <span class={`popover-anchor ${cls ?? ''}`} ref={root}>
      <button
        type="button"
        class={`tool small pop-btn ${open ? 'on' : ''}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        data-tip={open ? undefined : tip}
        aria-label={label ? undefined : tip}
        onClick={() => {
          if (fixed && root.current && !open) {
            const r = root.current.getBoundingClientRect();
            setAt(align === 'right' ? { top: r.bottom + 6, right: window.innerWidth - r.right } : { top: r.bottom + 6, left: r.left });
          }
          setOpen(!open);
        }}
      >
        {icon && <Icon name={icon} size={14} />}
        {label}
        {badge}
      </button>
      {open && (
        <div class={`popover ${align} ${fixed ? 'fixed' : ''}`} role="dialog" aria-label={tip} style={fixed && at ? at : undefined}>
          {typeof children === 'function' ? children(() => setOpen(false)) : children}
        </div>
      )}
    </span>
  );
}

/** One of a few choices, side by side: a segmented control. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label?: string; icon?: IconName; tip?: string; keys?: string; testid?: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div class="seg" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          class={`seg-btn ${value === o.value ? 'on' : ''}`}
          data-tip={o.tip}
          data-tip-keys={o.keys}
          data-testid={o.testid}
          aria-label={o.label ? undefined : (o.tip ?? o.value)}
          onClick={() => onChange(o.value)}
        >
          {o.icon && <Icon name={o.icon} size={14} />}
          {o.label && <span class="seg-label">{o.label}</span>}
        </button>
      ))}
    </div>
  );
}

/** A section that folds: a header with a chevron and a count, and its content. */
export function Disclosure({
  title,
  count,
  open: initial = true,
  children,
  right,
  class: cls,
}: {
  title: string;
  count?: ComponentChildren;
  open?: boolean;
  children: ComponentChildren;
  right?: ComponentChildren;
  class?: string;
}) {
  const [open, setOpen] = useState(initial);
  return (
    <section class={`disc ${open ? 'open' : ''} ${cls ?? ''}`}>
      <div class="disc-head">
        <button type="button" class="disc-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          <Icon name="chevron-right" size={12} class="disc-chev" />
          <span class="disc-title">{title}</span>
          {count !== undefined && <span class="disc-count">{count}</span>}
        </button>
        {right && <span class="disc-right">{right}</span>}
      </div>
      {open && <div class="disc-body">{children}</div>}
    </section>
  );
}

/** `3 min ago` for a time in the past, kept fresh while shown. */
export function useNow(every = 15000): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(t);
  }, [every]);
  return now;
}

export function ago(then: number, now: number): string {
  const s = Math.max(0, Math.round((now - then) / 1000));
  if (s < 10) return 'just now';
  if (s < 60) return `${s} s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  return h < 24 ? `${h} h ago` : `${Math.round(h / 24)} d ago`;
}

/** A quantity with its unit muted: `-50 mA .. 0 A` is the numbers in the page's ink and the units and dots fainter. */
export function Units({ text: raw }: { text: string }) {
  // `Range.text` has no space before the unit (the language's own writing); the page shows a thin space.
  const text = spaceUnits(raw);
  const out: ComponentChildren[] = [];
  let last = 0;
  let n = 0;
  for (const m of text.matchAll(/(-?\d+(?:\.\d+)?(?:e[-+]?\d+)?)(\s*[pnu\u00b5mkM]?[AV\u03a9W]\b)?/g)) {
    const at = m.index ?? 0;
    if (at > last) out.push(<span key={n++} class="u-sep">{text.slice(last, at)}</span>);
    out.push(<span key={n++} class="u-n">{m[1]}</span>);
    if (m[2]) out.push(<span key={n++} class="u-u">{m[2]}</span>);
    last = at + m[0].length;
  }
  if (last < text.length) out.push(<span key={n++} class="u-sep">{text.slice(last)}</span>);
  return <>{out}</>;
}

/** What a line in a drawing means: a small sample of it and a word. */
export function Legend({ items }: { items: { kind: string; label: string; tip?: string }[] }) {
  return (
    <span class="legend" role="list" aria-label="Legend">
      {items.map((i) => (
        <span class="lg" role="listitem" key={i.kind} data-tip={i.tip}>
          <svg width="20" height="8" viewBox="0 0 20 8" aria-hidden="true">
            <line class={`lg-line ${i.kind}`} x1="1" y1="4" x2="19" y2="4" />
          </svg>
          {i.label}
        </span>
      ))}
    </span>
  );
}

/** The one pattern of a view with nothing to show (nothing built, nothing found, nothing matches, no server, no KiCad): a mark
 * that says what kind of moment it is (neutral, ok, warn, bad), a title, one line of help, and the one thing to do. `compact`
 * is the size for a panel, a list or the dock; the default fills a view. The tone of the words is calm: what is so, and what
 * to do next, never an apology. */
export function EmptyState({
  icon,
  tone = 'neutral',
  title,
  help,
  children,
  actions,
  compact,
  class: cls,
  testid,
}: {
  icon: IconName;
  tone?: 'neutral' | 'ok' | 'warn' | 'bad';
  title: ComponentChildren;
  help?: ComponentChildren;
  /** Between the help and the actions: the error that stops it. */
  children?: ComponentChildren;
  actions?: ComponentChildren;
  compact?: boolean;
  class?: string;
  testid?: string;
}) {
  return (
    <div class={`empty ${compact ? 'compact' : ''} ${cls ?? ''}`} role="status" data-testid={testid}>
      <div class={`empty-mark ${tone}`} aria-hidden="true">
        <Icon name={icon} size={compact ? 16 : 22} />
      </div>
      <div class="empty-title">{title}</div>
      {help && <div class="empty-sub">{help}</div>}
      {children}
      {actions && <div class="empty-actions">{actions}</div>}
    </div>
  );
}
