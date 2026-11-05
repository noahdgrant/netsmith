// What the schematic has selected: the ONE place the schematic reads or writes a selection.
//
// The selection is the editor's shared one (`../selection.ts`: kind part|net|module, the key is
// the instance path of a part or module and the name of a net), so a pick in go to anything, the
// problems dock, the PCB, the bill of materials or the source's tree shows here, and a pick here
// shows there. The schematic itself thinks in designators (its drawing is keyed by them), so this
// module turns the key into the designator and back.

import { batch, computed, effect, signal } from '@preact/signals';

import { scene, select as selectOnPcb } from '../pcbstore';
import type { Bus } from '../schematic/drawing';
import { moduleSel, netSel, partByPath, selectAndReveal, selection, setSelection, under } from '../selection';
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

/** What a bus is for the selection: its instance, its definition and its member nets. */
export type PickedBus = Pick<Bus, 'path' | 'type' | 'nets' | 'members'>;

/** The bus the person picked, if the selection still is it.
 *
 * A bus is no kind of the page's shared selection: it is a drawing's thing (an interface instance
 * drawn as one line, named differently at its two ends), so the other views have no use for a kind of
 * their own. Picking one selects the shared selection's net, the bus's first member (the PCB lights
 * that net, the chip and the inspector know better), and remembers the bus here. This is valid only
 * while the selection is still that net: any other pick, Esc, or a click on one of the nets drops it. */
const pickedBus = signal<PickedBus | null>(null);
export const selectedBus = computed<PickedBus | null>(() => {
  const b = pickedBus.value;
  const s = selection.value;
  return b && s?.kind === 'net' && s.key === b.nets[0] ? b : null;
});
effect(() => {
  // The selection moved off the bus's net: the bus is not picked any more.
  const s = selection.value;
  const b = pickedBus.peek();
  if (b && !(s?.kind === 'net' && s.key === b.nets[0])) pickedBus.value = null;
});

/** Every bus the schematic of `board` draws: what go to anything lists. */
export const drawnBuses = signal<{ board: string; buses: Bus[] }>({ board: '', buses: [] });

/** Pick a bus: all its members are lit in the schematic, the first one is the shared selection. */
export function selectBus(bus: PickedBus) {
  batch(() => {
    pickedBus.value = { path: bus.path, type: bus.type, nets: bus.nets, members: bus.members };
    setSelection(netSel(bus.nets[0]));
  });
}

/** Pick a bus and ask the views to bring it into view (go to anything); the Schematic frames its line. */
export function goToBus(bus: PickedBus) {
  batch(() => {
    pickedBus.value = { path: bus.path, type: bus.type, nets: bus.nets, members: bus.members };
    selectAndReveal(netSel(bus.nets[0]));
  });
}

export const selectPart = (refdes: string) => pickPart(refdes);
/** Pick a net: a bus that was picked is not any more, also when the net is its first member. */
export const selectNet = (name: string) => {
  pickedBus.value = null;
  pickNet(name);
};
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
