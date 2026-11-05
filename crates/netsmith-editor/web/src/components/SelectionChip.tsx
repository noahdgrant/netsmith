// What is selected, in the top bar, whatever view is open: its kind, its name, a way to its
// source, and a way to clear it. The selection is the page's (selection.ts): this only shows it.

import { chordText, find } from '../commands';
import { declarationOf, describe, selection, selectAndReveal, setSelection } from '../selection';
import { detail, showInSource } from '../store';

export function SelectionChip() {
  const sel = selection.value;
  if (!sel) return null;
  const d = detail.value;
  const { title, sub } = describe(d, sel);
  const has = declarationOf(d, sel) !== null;
  const key = find('selection.source')?.keys;
  return (
    <div class={`selchip k-${sel.kind}`} role="status" aria-label={`Selected ${sub}: ${title}`}>
      <button class="selchip-main" title={`${sub}. Click to show it again in this view.`} onClick={() => selectAndReveal(sel)}>
        <span class={`badge k-${sel.kind}`}>{sel.kind === 'part' ? 'P' : sel.kind === 'net' ? 'N' : 'M'}</span>
        <span class="selchip-title">{title}</span>
        <span class="selchip-sub">{sub}</span>
      </button>
      {has && (
        <button class="selchip-btn" title={`Show it in Source${key ? ` (${chordText(key)})` : ''}`} onClick={() => showInSource(sel)}>
          Source
        </button>
      )}
      <button class="selchip-btn" title="Clear the selection (Esc)" aria-label="Clear the selection" onClick={() => setSelection(null)}>
        ×
      </button>
    </div>
  );
}
