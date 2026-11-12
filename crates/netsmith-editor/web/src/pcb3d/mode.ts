// Whether the PCB view draws the board in 2D or in 3D, and what the 3D view lets the commands do
// while it is on screen. This file is in the page's main chunk; the 3D view itself (three.js and
// everything that draws) is a chunk of its own, loaded the first time 3D is opened (ADR 0105).

import { signal } from '@preact/signals';

export type PcbMode = '2d' | '3d';

export type Preset = 'top' | 'bottom' | 'front' | 'back' | 'left' | 'right' | 'iso';

/** The camera's preset views, in the order of the 3D view's bar; the main three have a key (the
 * digits are the sections'). The camera's numbers for each are in presets.ts, in the 3D view's own chunk. */
export const PRESETS: { id: Preset; label: string; key?: string }[] = [
  { id: 'top', label: 'Top', key: 'T' },
  { id: 'bottom', label: 'Bottom', key: 'B' },
  { id: 'front', label: 'Front' },
  { id: 'back', label: 'Back' },
  { id: 'left', label: 'Left' },
  { id: 'right', label: 'Right' },
  { id: 'iso', label: 'Isometric', key: 'I' },
];

/** 2D or 3D, for the session (a reload opens 2D, where the board is edited). */
export const pcbMode = signal<PcbMode>('2d');

export function togglePcbMode() {
  pcbMode.value = pcbMode.peek() === '3d' ? '2d' : '3d';
}

/** What the 3D view does for a command or a button. */
export interface View3dActions {
  preset(p: Preset): void;
  flip(): void;
  toggleProjection(): void;
  fit(): void;
}

/** The actions of the 3D view while it is mounted, else null. */
export const view3d: { current: View3dActions | null } = { current: null };

/** The 3D view's chunk, loaded once. */
export const load3d = () => import('./View3d');
