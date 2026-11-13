// Whether the PCB view draws the board in 2D or in 3D, and what the 3D view lets the commands do
// while it is on screen. This file is in the page's main chunk; the 3D view itself (three.js and
// everything that draws) is a chunk of its own, loaded the first time 3D is opened (ADR 0105).

import { signal } from '@preact/signals';

import type { ViewChange, ViewState } from '../api';

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

/** The system whose _System scene_ the 3D view shows in place of the board (ADR 0110): its name,
 * while the board is one of its members; null shows the board alone. */
export const systemShown = signal<string | null>(null);

export function togglePcbMode() {
  pcbMode.value = pcbMode.peek() === '3d' ? '2d' : '3d';
}

/** An axis of the scene's frame, for the section plane. */
export type CutAxis = 'x' | 'y' | 'z';

/** What the 3D view does for a command or a button. */
export interface View3dActions {
  preset(p: Preset): void;
  flip(): void;
  toggleProjection(): void;
  fit(): void;
  /** The measure tool on or off (the key M, which the view handles: the schematic's M mirrors). */
  toggleMeasure(): void;
  clearMeasures(): void;
  /** The section plane on or off, keeping its axis and place. */
  toggleSection(): void;
  /** The section plane across an axis. */
  section(axis: CutAxis): void;
  /** The stackup pulled apart, or put back. */
  toggleExplode(): void;
  /** Copy the `netsmith render` command that draws the view. */
  copyCommand(): void;
  /** What `/api/view` drives (agent.ts). */
  agent: View3dAgent;
}

/** The 3D view as an agent drives it over `/api/view` (agent.ts, control.ts). */
export interface View3dAgent {
  /** The board whose scene is drawn, once it is; null before. */
  loaded(): string | null;
  /** Why the view cannot draw (no WebGL 2, a scene that did not load), else null. */
  failed(): string | null;
  /** Check a change against the scene and make it, as the keys and the palette would; it ends
   * once the view has drawn it. A change it refuses (a `ViewRefusal`) changes nothing. */
  apply(change: ViewChange): Promise<void>;
  /** The camera, the cut, the stackup and what is lit, as `/api/view` answers them. */
  state(): Pick<ViewState, 'view' | 'camera' | 'ortho' | 'section' | 'exploded' | 'command'> & { limits: string[]; cases: string[] };
  /** A PNG of the canvas, drawn for it. */
  picture(): Promise<Blob>;
  /** The designator of a part of the scene by its path (`led/power_in` in a system's), else null. */
  refdes(path: string): string | null;
}

/** A change or a question of `/api/view` the page refuses, with the HTTP status the route answers with. */
export class ViewRefusal extends Error {
  constructor(
    message: string,
    readonly status = 400,
  ) {
    super(message);
  }
}

/** The actions of the 3D view while it is mounted, else null. */
export const view3d: { current: View3dActions | null } = { current: null };

/** The 3D view's chunk, loaded once. */
export const load3d = () => import('./View3d');
