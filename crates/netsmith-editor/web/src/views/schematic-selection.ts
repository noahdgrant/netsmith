// What the schematic has selected: the ONE place the schematic reads or writes a selection.
//
// Today a selected part is the store's `selected` (a designator) and a selected net is kept here.
// When the editor has one selection shared by every view (kind part|net|module, key = the
// instance path or the net name), swap the bodies below for it and nothing else in the schematic
// changes: `Schematic.tsx` and its panels import only from this module.

import { computed, effect, signal } from '@preact/signals';

import { select as selectOnPcb } from '../pcbstore';
import { board, detail, lineParts, pickPart, revealSection, selected } from '../store';

/** The net selected in the schematic: it stays lit, with everything else dimmed, until cleared. */
const net = signal<string | null>(null);

/** What is selected. */
export const selection = computed<{ kind: 'part' | 'net'; key: string } | null>(() => {
  if (net.value) return { kind: 'net', key: net.value };
  if (selected.value) return { kind: 'part', key: selected.value };
  return null;
});
/** The selected part's designator, or null. */
export const selectedPart = computed(() => (net.value ? null : selected.value));
/** The selected net's name, or null. */
export const selectedNet = computed(() => net.value);
/** The designators of the parts the source's cursor line declares (they are marked, not selected). */
export const cursorParts = computed(() => lineParts.value);

export function selectPart(refdes: string) {
  net.value = null;
  selected.value = refdes;
}

export function selectNet(name: string) {
  selected.value = null;
  net.value = name;
}

export function clearSelection() {
  net.value = null;
  selected.value = null;
}

/** Open the source at the part's declaration. */
export function goToSource(refdes: string) {
  pickPart(refdes, true);
}

/** Show the part in the PCB view, selected. */
export function showOnPcb(refdes: string) {
  const part = detail.peek()?.parts.find((p) => p.refdes === refdes);
  if (!part) return;
  selectOnPcb([part.path]);
  revealSection('pcb');
}

// Another board has none of this board's nets.
effect(() => {
  void board.value;
  net.value = null;
});
