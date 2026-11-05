// What the Schematic view lets the command registry do. The view owns the zoom, the sheet shown
// and the walk of a net's pins, so it hands them out here while it is mounted; the commands of
// the Schematic group (`commandset.ts`) call them, and so do the view's buttons. A command exists
// once: the keys, the palette, the cheat sheet, the toolbar and the chip all run these commands.

import { signal } from '@preact/signals';

export interface SchematicActions {
  /** Whether every sheet is shown at once (the overview), where there is nothing to zoom. */
  overview(): boolean;
  /** How many sheets the board has. */
  sheets(): number;
  /** The previous (-1) or next (1) sheet, round the ends. */
  sheet(dir: 1 | -1): void;
  fit(): void;
  fitWidth(): void;
  /** 100%: the drawing's size on paper. */
  actual(): void;
  /** Zoom to the selected part (with what it is wired to) or net. */
  zoomSelection(): void;
  zoom(factor: number): void;
  pan(dx: number, dy: number): void;
  /** The next (1) or previous (-1) pin of the selected net, which opens its sheet. */
  pin(dir: 1 | -1): void;
  /** Open the selected part in the source. */
  source(): void;
}

/** The view's hands, while it is mounted; null otherwise. */
export const schematicActions: { current: SchematicActions | null } = { current: null };

/** What the top bar's chip says of the selected net, which only the drawing knows: how many pins
 * and on how many sheets, and whether there is more than one pin to step to. */
export interface NetFacts {
  net: string;
  text: string;
  canStep: boolean;
}
export const netFacts = signal<NetFacts | null>(null);

/** How far an arrow key pans, in screen pixels. */
export const PAN_STEP = 80;
