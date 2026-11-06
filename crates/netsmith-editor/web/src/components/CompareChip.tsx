// The status bar's chip for the comparison: `vs main: 5 changes`, once a ref is chosen. A click opens a small
// menu to look at it, to mark the views, to compare with another ref or to stop.

import { useEffect, useRef, useState } from 'preact/hooks';

import { changes, clearCompare, compare, refName, refText, runCompare, showChanges, state } from '../compare/store';
import { plural } from '../util';
import { find, chordText } from '../commands';
import { askForRef, toggleChanges } from './ChangesSwitch';
import { Icon, type IconName } from './Icons';
import { revealSection } from '../store';

function Menu({ close }: { close: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    box.current?.querySelector<HTMLElement>('button')?.focus();
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node) && !(e.target as Element).closest?.('.cmp-chip-anchor')) close();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        close();
      }
    };
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('keydown', key, true);
    };
  }, [close]);
  const s = state.value;
  const c = compare.value;
  const row = (label: string, onClick: () => void, opts: { checked?: boolean; keys?: string; id?: string; icon?: IconName } = {}) => (
    <button type="button" class="src-opt" role={opts.checked === undefined ? 'menuitem' : 'menuitemcheckbox'} aria-checked={opts.checked} data-testid={opts.id} onClick={() => (close(), onClick())}>
      {opts.checked === undefined ? (
        <span class="src-ic" aria-hidden="true">{opts.icon && <Icon name={opts.icon} size={14} />}</span>
      ) : (
        <span class="src-check" aria-hidden="true">{opts.checked ? <Icon name="check" size={12} /> : null}</span>
      )}
      {label}
      {opts.keys && <kbd>{chordText(opts.keys)}</kbd>}
    </button>
  );
  return (
    <div class="status-popover cmp-chip-menu" ref={box} role="dialog" aria-label="Comparison with a git ref" data-esc-closes>
      <div class="status-popover-title">Compared with {refText.value}</div>
      {c && <p class="cmp-chip-sentence">{c.summary}</p>}
      {s.kind === 'ready' && s.data.error && <p class="cmp-chip-sentence">{s.data.error}</p>}
      {s.kind === 'failed' && <p class="cmp-chip-sentence">{s.message}</p>}
      <div class="src-opts" role="menu" aria-label="Comparison">
        {row('Show the comparison', () => revealSection('compare'), { keys: find('view.compare')?.keys, id: 'chip-show', icon: 'compare' })}
        {row('Show changes in the views', toggleChanges, { checked: showChanges.value, id: 'chip-marks' })}
        {row('Compare with another ref…', askForRef, { id: 'chip-another', icon: 'branch' })}
        {row('Compare again', () => refName.value && void runCompare(refName.value), { icon: 'refresh' })}
        {row('Stop comparing', () => (clearCompare(), (showChanges.value = false)), { id: 'chip-stop', icon: 'close' })}
      </div>
    </div>
  );
}

export function CompareChip() {
  const ref = refName.value;
  const [open, setOpen] = useState(false);
  const s = state.value;
  // Nothing yet to say about a ref that is remembered and not asked for.
  if (!ref || s.kind === 'idle') return null;
  const n = changes.value;
  const text =
    s.kind === 'loading'
      ? 'comparing…'
      : s.kind === 'failed' || (s.kind === 'ready' && s.data.error)
        ? 'cannot compare'
        : s.kind === 'ready'
          ? n === 0
            ? 'no changes'
            : plural(n, 'change')
          : '';
  const bad = s.kind === 'failed' || (s.kind === 'ready' && !!s.data.error);
  return (
    <span class={`status-anchor cmp-chip-anchor ${open ? 'open' : ''}`}>
      <button
        class={`item cmp-chip ${bad ? 'bad' : ''} ${showChanges.value ? 'marking' : ''}`}
        data-testid="compare-chip"
        data-tip={`Compared with ${refText.value}. Click for the comparison and for marking the views.`}
        aria-label={`Compared with ${refText.value}: ${text}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {s.kind === 'loading' ? <span class="spinner" aria-hidden="true" /> : <Icon name={bad ? 'warning' : 'compare'} size={13} />}
        <span class="cmp-chip-text">
          <span class="cmp-vs">
            vs <span class="mono">{refText.value}</span>:{' '}
          </span>
          {text}
        </span>
      </button>
      {open && <Menu close={() => setOpen(false)} />}
    </span>
  );
}
