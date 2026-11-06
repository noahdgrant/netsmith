// What is selected, in the top bar, whatever view is open: its kind, its name, a way to its
// source, and a way to clear it. The selection is the page's (selection.ts): this only shows it.

import { chordText, find, runCommand, withKey } from '../commands';
import { declarationOf, describe, selection, selectAndReveal, setSelection } from '../selection';
import { detail, layout, showInSource, visible } from '../store';
import { busChip } from '../schematic/bus';
import { Icon } from './Icons';
import { netFacts } from '../views/schematic-actions';
import { selectedBus } from '../views/schematic-selection';

export function SelectionChip() {
  const sel = selection.value;
  if (!sel) return null;
  const d = detail.value;
  // A bus is picked as its first net (the other views light that net); the chip says the bus.
  const bus = selectedBus.value;
  const { title, sub: plain } = bus ? busChip(bus) : describe(d, sel);
  // A net: how many pins on how many sheets, which only the schematic's drawing knows, and
  // buttons to walk the pins while the Schematic is the view in use (the commands Tab and
  // Shift+Tab run).
  const facts = !bus && sel.kind === 'net' && netFacts.value?.net === sel.key ? netFacts.value : null;
  const sub = facts ? facts.text : plain;
  const vis = visible.value;
  const here = (vis.find((p) => p.index === layout.value.focus) ?? vis[0])?.section === 'schematic';
  const walk = sel.kind === 'net' && here && facts !== null;
  const has = declarationOf(d, sel) !== null;
  const key = find('selection.source')?.keys;
  return (
    <div class={`selchip k-${bus ? 'bus' : sel.kind}`} role="status" aria-label={bus ? `Selected ${title} ${sub}` : `Selected ${sub}: ${title}`}>
      <button class="selchip-main" data-tip={`${bus ? `${bus.type} ${bus.path} ${sub}` : sub}. Click to show it again in this view.`} aria-label={`${title}, ${sub}. Show it again in this view.`} onClick={() => selectAndReveal(sel)}>
        <span class={`selchip-kind k-${bus ? 'bus' : sel.kind}`} aria-hidden="true">
          <Icon name={bus ? 'bus' : sel.kind === 'part' ? 'part' : sel.kind === 'net' ? 'net' : 'module'} size={14} />
        </span>
        <span class="selchip-title">{title}</span>
        <span class={`selchip-sub ${facts || bus ? 'keep' : ''}`}>{sub}</span>
      </button>
      {walk && (
        <>
          <button
            class="selchip-btn step"
            disabled={!facts.canStep}
            data-tip={withKey('Previous pin of the net', 'schematic.pin.previous')}
            aria-label="Previous pin"
            onClick={() => runCommand('schematic.pin.previous')}
          >
            <Icon name="chevron-left" size={14} />
          </button>
          <button
            class="selchip-btn step"
            disabled={!facts.canStep}
            data-tip={withKey('Next pin of the net', 'schematic.pin.next')}
            aria-label="Next pin"
            onClick={() => runCommand('schematic.pin.next')}
          >
            <Icon name="chevron-right" size={14} />
          </button>
        </>
      )}
      {has && (
        <button class="selchip-btn text" data-tip={`Show it in Source${key ? ` (${chordText(key)})` : ''}`} onClick={() => showInSource(sel)}>
          <Icon name="source" size={13} />
          Source
        </button>
      )}
      <button class="selchip-btn" data-tip="Clear the selection" data-tip-keys="Esc" aria-label="Clear the selection" onClick={() => setSelection(null)}>
        <Icon name="close" size={13} />
      </button>
    </div>
  );
}
