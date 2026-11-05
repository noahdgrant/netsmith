// What is selected, in the top bar, whatever view is open: its kind, its name, a way to its
// source, and a way to clear it. The selection is the page's (selection.ts): this only shows it.

import { chordText, find, runCommand, withKey } from '../commands';
import { declarationOf, describe, selection, selectAndReveal, setSelection } from '../selection';
import { detail, layout, showInSource, visible } from '../store';
import { netFacts } from '../views/schematic-actions';

export function SelectionChip() {
  const sel = selection.value;
  if (!sel) return null;
  const d = detail.value;
  const { title, sub: plain } = describe(d, sel);
  // A net: how many pins on how many sheets, which only the schematic's drawing knows, and
  // buttons to walk the pins while the Schematic is the view in use (the commands Tab and
  // Shift+Tab run).
  const facts = sel.kind === 'net' && netFacts.value?.net === sel.key ? netFacts.value : null;
  const sub = facts ? facts.text : plain;
  const vis = visible.value;
  const here = (vis.find((p) => p.index === layout.value.focus) ?? vis[0])?.section === 'schematic';
  const walk = sel.kind === 'net' && here && facts !== null;
  const has = declarationOf(d, sel) !== null;
  const key = find('selection.source')?.keys;
  return (
    <div class={`selchip k-${sel.kind}`} role="status" aria-label={`Selected ${sub}: ${title}`}>
      <button class="selchip-main" title={`${sub}. Click to show it again in this view.`} onClick={() => selectAndReveal(sel)}>
        <span class={`badge k-${sel.kind}`}>{sel.kind === 'part' ? 'P' : sel.kind === 'net' ? 'N' : 'M'}</span>
        <span class="selchip-title">{title}</span>
        <span class={`selchip-sub ${facts ? 'keep' : ''}`}>{sub}</span>
      </button>
      {walk && (
        <>
          <button
            class="selchip-btn step"
            disabled={!facts.canStep}
            title={withKey('Previous pin of the net', 'schematic.pin.previous')}
            aria-label="Previous pin"
            onClick={() => runCommand('schematic.pin.previous')}
          >
            ‹
          </button>
          <button
            class="selchip-btn step"
            disabled={!facts.canStep}
            title={withKey('Next pin of the net', 'schematic.pin.next')}
            aria-label="Next pin"
            onClick={() => runCommand('schematic.pin.next')}
          >
            ›
          </button>
        </>
      )}
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
