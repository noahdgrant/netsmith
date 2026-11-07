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
  /** Arranging by hand (ADR 0061): the layout file, through the one writer. */
  layout: LayoutActions;
}

export interface LayoutActions {
  /** A part is selected and its block can be moved: the file reads, and it is not the sheet's anchor. */
  canMove(): boolean;
  /** A part is selected and can be turned or mirrored (the anchor can). */
  canTurn(): boolean;
  /** Move the selected part's block by grid steps. */
  nudge(sx: number, sy: number): void;
  /** A quarter turn counterclockwise (1) or clockwise (-1) of the selected part. */
  turn(dir: 1 | -1): void;
  mirror(): void;
  /** The selected part has an entry in the file. */
  hinted(): boolean;
  /** Remove the selected part's entry. */
  clearPart(): void;
  /** How many entries the module on show has, and the file (every board). */
  sheetEntries(): number;
  allEntries(): number;
  /** Ask, then remove every entry of the module on show, or of the file. */
  clearSheet(): void;
  clearAll(): void;
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

/** How many grid steps an arrow key nudges a part, and Shift+arrow. */
export const NUDGE = 1;
export const NUDGE_FAR = 10;

/** How far an arrow key pans, in screen pixels. */
export const PAN_STEP = 80;
