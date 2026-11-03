// What the keys and buttons of the PCB canvas do to the selection. Each is one step of undo.

import type { PcbChange, PcbKeepout, PcbScene } from '../api';
import { commit, grid, keepoutDefaults, pickedKeepout, scene, select, selection } from '../pcbstore';
import { say } from '../store';
import {
  type Frame,
  drawKeepout,
  entryOf,
  flipParts,
  keepoutEntry,
  lockParts,
  moveParts,
  releaseParts,
  rotateParts,
} from './edits';
import { tidy } from './geometry';

export function frameOf(sc: PcbScene): Frame {
  return { originX: sc.originX, originY: sc.originY };
}

function chosen() {
  const sc = scene.value;
  if (!sc) return null;
  const parts = sc.parts.filter((p) => selection.value.includes(p.path));
  return parts.length ? { sc, parts, frame: frameOf(sc) } : null;
}

const names = (parts: { refdes: string }[]) =>
  parts.length === 1 ? parts[0].refdes : `${parts.length} parts`;

/** Run an edit on the selection; say why when it does nothing because every part is locked. */
async function edit(
  make: (c: NonNullable<ReturnType<typeof chosen>>) => PcbChange[],
  label: (parts: { refdes: string }[]) => string,
) {
  const c = chosen();
  if (!c) return;
  const changes = make(c);
  if (changes.length === 0) {
    if (c.parts.every((p) => p.locked)) {
      say(`${names(c.parts)} ${c.parts.length === 1 ? 'is' : 'are'} locked: unlock with L`, true);
    }
    return;
  }
  const done = c.parts.filter((p) => changes.some((ch) => ch.what === 'part' && ch.path === p.path));
  await commit(changes, label(done));
}

export const rotateSelection = (degrees: number) =>
  edit(
    (c) => rotateParts(c.parts, c.frame, degrees),
    (p) => `rotate ${names(p)}`,
  );

export const flipSelection = () =>
  edit(
    (c) => flipParts(c.parts, c.frame),
    (p) => `flip ${names(p)}`,
  );

/** Lock the selection, or unlock it if all of it is locked. */
export const toggleLock = () => {
  const c = chosen();
  if (!c) return Promise.resolve();
  const lock = !c.parts.every((p) => p.locked);
  return commit(lockParts(c.parts, c.frame, lock), `${lock ? 'lock' : 'unlock'} ${names(c.parts)}`);
};

export const releaseSelection = () =>
  edit(
    (c) => releaseParts(c.parts, c.frame),
    (p) => `let the placer place ${names(p)}`,
  );

/** Move the selection by one step of the grid (a tenth of a millimetre with no grid). */
export const nudge = (dx: number, dy: number, big: boolean) => {
  const step = (grid.value || 0.1) * (big ? 10 : 1);
  return edit(
    (c) => moveParts(c.parts, c.frame, { x: tidy(dx * step), y: tidy(dy * step) }),
    (p) => `nudge ${names(p)}`,
  );
};

export function selectAll() {
  const sc = scene.value;
  if (sc) select(sc.parts.map((p) => p.path));
}

/** The first name `keepout1`, `keepout2`... that the board does not use. */
export function freeKeepoutName(sc: PcbScene): string {
  for (let n = 1; ; n++) {
    const name = `keepout${n}`;
    if (!sc.keepouts.some((k) => k.name === name)) return name;
  }
}

/** Draw a keepout over `box` (on the sheet) with what the toolbar says it forbids. */
export async function addKeepout(box: { x: number; y: number; width: number; height: number }): Promise<string | null> {
  const sc = scene.value;
  if (!sc) return null;
  const d = keepoutDefaults.value;
  const name = freeKeepoutName(sc);
  const layers = d.layers === 'all' ? sc.layers : [d.layers];
  const ok = await commit(
    [drawKeepout(name, box, frameOf(sc), { layers, tracks: d.tracks, vias: d.vias, copper: d.copper, parts: d.parts })],
    `draw keepout ${name}`,
  );
  return ok ? name : null;
}

/** Change a keepout that was drawn or placed: where it is and, if drawn, what it is. */
export async function changeKeepout(k: PcbKeepout, to: Partial<PcbKeepout>, label: string) {
  const sc = scene.value;
  if (!sc) return false;
  const f = frameOf(sc);
  const before = keepoutEntry(k, f);
  const after = keepoutEntry({ ...k, ...to }, f);
  if (!before || !after) return false;
  return commit([{ what: 'keepout', name: k.name, before, after }], label);
}

/** Give a keepout the source sizes and not places a position, in the middle of the board. */
export async function placeKeepout(name: string) {
  const sc = scene.value;
  const k = sc?.keepouts.find((x) => x.name === name);
  if (!sc || !k || k.origin !== 'placed' || k.area) return false;
  const x = tidy(Math.max(0, (sc.outline.width - k.width) / 2));
  const y = tidy(Math.max(0, (sc.outline.height - k.height) / 2));
  return commit([{ what: 'keepout', name, before: null, after: { x, y, drawn: null } }], `place keepout ${name}`);
}

/** Take a drawn keepout away. */
export async function deleteKeepout(name: string) {
  const sc = scene.value;
  const k = sc?.keepouts.find((x) => x.name === name);
  if (!sc || !k || k.origin !== 'drawn') return false;
  const before = keepoutEntry(k, frameOf(sc));
  if (!before) return false;
  const ok = await commit([{ what: 'keepout', name, before, after: null }], `delete keepout ${name}`);
  if (ok && pickedKeepout.value === name) pickedKeepout.value = null;
  return ok;
}

export { entryOf };
