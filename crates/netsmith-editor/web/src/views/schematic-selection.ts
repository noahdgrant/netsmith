// What the schematic has selected: the ONE place the schematic reads or writes a selection.
//
// The selection is the editor's shared one (`../selection.ts`: kind part|net|module, the key is
// the instance path of a part or module and the name of a net), so a pick in go to anything, the
// problems dock, the PCB, the bill of materials or the source's tree shows here, and a pick here
// shows there. The schematic itself thinks in designators (its drawing is keyed by them), so this
// module turns the key into the designator and back.

import { computed } from '@preact/signals';

import { scene, select as selectOnPcb } from '../pcbstore';
import { moduleSel, partByPath, selection, setSelection, under } from '../selection';
import { board, detail, lineParts, pickNet, pickPart, revealSection } from '../store';

export { selection };

/** The selected part's designator, or null (also null while the board's detail is not read). */
export const selectedPart = computed(() => {
  const s = selection.value;
  return s?.kind === 'part' ? (partByPath(detail.value, s.key)?.refdes ?? null) : null;
});
/** The selected net's name, or null. */
export const selectedNet = computed(() => (selection.value?.kind === 'net' ? selection.value.key : null));
/** The selected module's instance path (`stage1`), or null. */
export const selectedModule = computed(() => (selection.value?.kind === 'module' ? selection.value.key : null));
/** The designators of the parts under the selected module (they are marked together). */
export const moduleParts = computed(() => {
  const m = selectedModule.value;
  return m ? (detail.value?.parts ?? []).filter((p) => under(p.path, m)).map((p) => p.refdes) : [];
});
/** The designators of the parts the source's cursor line declares (they are marked, not selected). */
export const cursorParts = computed(() => lineParts.value);

export const selectPart = (refdes: string) => pickPart(refdes);
export const selectNet = (name: string) => pickNet(name);
export const selectModule = (path: string) => setSelection(moduleSel(path));
export const clearSelection = () => setSelection(null);

/** Open the source at the part's declaration. */
export function goToSource(refdes: string) {
  pickPart(refdes, true);
}

/** Show the part in the PCB view, selected. */
export function showOnPcb(refdes: string) {
  const part = detail.peek()?.parts.find((p) => p.refdes === refdes);
  if (!part) return;
  const name = board.peek();
  revealSection('pcb');
  // A board the PCB view has not drawn yet starts with nothing selected: select when its scene is in.
  if (scene.peek()?.board === name) {
    selectOnPcb([part.path]);
    return;
  }
  const stop = scene.subscribe((s) => {
    if (s?.board !== name) return;
    queueMicrotask(() => {
      stop();
      selectOnPcb([part.path]);
    });
  });
  setTimeout(stop, 20000);
}
