// The 3D view's ways of looking (P4b of issue 430): x-ray, what the bodies are coloured by, the height
// preview, and a finding the problems dock asks the view to show. This file is in the page's main
// chunk, with the commands that set them (the palette and the keys work before the view's chunk is
// loaded); the drawing that reads them is in the view's own chunk (viewer.ts, inspect.ts).
//
// None of these is data. X-ray and the colouring change only how the scene is drawn. The height
// preview (ADR 0105: "preview at N mm") stands the parts of unknown height up in the picture only:
// no check, finding, report, written file or the height coverage reads it, and it is off again
// after a reload (it is kept nowhere).

import { signal } from '@preact/signals';

import type { Command } from '../commands';

/** What a part's body is coloured by: where its height came from (the default), its kind, or its margin to the height limit over it. */
export type ColourBy = 'provenance' | 'kind' | 'margin';

export const COLOUR_BY: { id: ColourBy; label: string; tip: string }[] = [
  { id: 'provenance', label: 'Height provenance', tip: 'Colour each body by where its height came from: stated in the design, the catalog row, or the package standard' },
  { id: 'kind', label: 'Kind', tip: 'Colour each body by the kind of part: resistor, capacitor, IC, connector…' },
  { id: 'margin', label: 'Height margin', tip: 'A height map: each body by its margin to the height limit over it, green to amber to red; grey where no limit is over it' },
];

/** How the case model (ADR 0109) is drawn, as `netsmith render --case` names it: not at all, see-through (the default) or solid. */
export type CaseMode = 'hidden' | 'see-through' | 'solid';

export const CASE_MODES: { id: CaseMode; label: string; tip: string }[] = [
  { id: 'hidden', label: 'Hidden', tip: 'Hide the case model: the board alone' },
  { id: 'see-through', label: 'See-through', tip: 'Draw the case model faint, so the board shows inside it' },
  { id: 'solid', label: 'Solid', tip: 'Draw the case model solid: it hides the board inside it' },
];

/** The case after the Case button: hidden when it shows, and what it last showed as (see-through unless it was solid) when it is hidden. */
export function caseAfterShowHide(mode: CaseMode, last: CaseMode): CaseMode {
  return mode === 'hidden' ? (last === 'hidden' ? 'see-through' : last) : 'hidden';
}

/** The case after the Solid button: solid, or see-through again when it was solid. A hidden case shows, solid. */
export function caseAfterSolid(mode: CaseMode): CaseMode {
  return mode === 'solid' ? 'see-through' : 'solid';
}

/** The key's step: see-through, solid, hidden, and round again. */
export function caseAfterStep(mode: CaseMode): CaseMode {
  return mode === 'see-through' ? 'solid' : mode === 'solid' ? 'hidden' : 'see-through';
}

/** The preview's height: 1 mm unless changed, from 0.1 to 50 mm. */
export const PREVIEW_DEFAULT_MM = 1;
export const PREVIEW_MIN_MM = 0.1;
export const PREVIEW_MAX_MM = 50;

/** A preview height from what a person typed, or null when it is not one. */
export function previewHeight(text: string): number | null {
  const v = Number(text.trim().replace(/\s*mm$/i, '').replace(',', '.'));
  if (!Number.isFinite(v) || v < PREVIEW_MIN_MM || v > PREVIEW_MAX_MM) return null;
  return Math.round(v * 100) / 100;
}

/** See-through board, mask and bodies. */
export const xray = signal(false);
export const colourBy = signal<ColourBy>('provenance');
/** How the case is drawn, for the session (the tools of this view are kept nowhere, as x-ray is not), see-through at every start. */
export const caseMode = signal<CaseMode>('see-through');
/** What the case last showed as, for the button that hides and shows it again. */
let caseLast: CaseMode = 'see-through';

/** Draw the case in a mode. */
export function setCase(mode: CaseMode) {
  if (mode !== 'hidden') caseLast = mode;
  caseMode.value = mode;
}

export function toggleCaseShown() {
  setCase(caseAfterShowHide(caseMode.peek(), caseLast));
}

export function toggleCaseSolid() {
  setCase(caseAfterSolid(caseMode.peek()));
}

export function stepCase() {
  setCase(caseAfterStep(caseMode.peek()));
}

/** The height preview: off at every start (it is never stored), at `mm` when on. */
export const heightPreview = signal<{ on: boolean; mm: number }>({ on: false, mm: PREVIEW_DEFAULT_MM });

/** A finding the problems dock picked, for the 3D view to fly to and light: its parts by designator, the limit it is about, and for a finding of the case fit (ADR 0109) the case solid (`case:<model>:<solid>`) and the point in the scene's frame. */
export const focus3d = signal<{ refs: string[]; keepout: string | null; caseSolid: string | null; point: [number, number, number] | null; nonce: number } | null>(null);

let asked = 0;

/** Ask the 3D view, when it is open, to fly to a finding and light it (the 2D board is asked its own way). */
export function askFocus3d(refs: string[], keepout: string | null | undefined, caseSolid?: string | null, point?: [number, number, number] | null) {
  focus3d.value = { refs, keepout: keepout ?? null, caseSolid: caseSolid ?? null, point: point ?? null, nonce: ++asked };
}

export function toggleXray() {
  xray.value = !xray.peek();
}

/** The next colouring, in the menu's order. */
export function cycleColourBy() {
  const i = COLOUR_BY.findIndex((c) => c.id === colourBy.peek());
  colourBy.value = COLOUR_BY[(i + 1) % COLOUR_BY.length].id;
}

export function togglePreview() {
  heightPreview.value = { ...heightPreview.peek(), on: !heightPreview.peek().on };
}

/** The commands of these, for the palette, the keys and the cheat sheet. `in3d` says the view is on screen; `here` that the PCB is the pane in use. */
export function inspectCommands(in3d: () => boolean, here: () => boolean): Command[] {
  return [
    {
      id: 'pcb.3d.xray',
      title: '3D view: X-ray (see through the board and the bodies)',
      group: 'PCB',
      // Not X: the section plane (P4a) picks its axis with X, Y and Z.
      keys: 'V',
      aliases: ['3d', 'xray', 'x-ray', 'transparent', 'see through', 'inner layers', 'vias'],
      keyFrom: here,
      when: in3d,
      run: toggleXray,
    },
    {
      id: 'pcb.3d.colour.next',
      title: '3D view: Colour the bodies by the next of height provenance, kind, height margin',
      group: 'PCB',
      keys: 'C',
      aliases: ['3d', 'colour by', 'color by', 'kind', 'height map', 'margin', 'provenance'],
      keyFrom: here,
      when: in3d,
      run: cycleColourBy,
    },
    ...COLOUR_BY.map<Command>((c) => ({
      id: `pcb.3d.colour.${c.id}`,
      title: `3D view: Colour the bodies by ${c.label.toLowerCase()}`,
      group: 'PCB',
      aliases: ['3d', 'colour by', 'color by', c.id === 'margin' ? 'height map' : c.id],
      when: in3d,
      run: () => {
        colourBy.value = c.id;
      },
    })),
    {
      id: 'pcb.3d.case.next',
      title: '3D view: Case model: step through see-through, solid and hidden',
      group: 'PCB',
      // Shift+C: C is the colouring's.
      keys: 'Shift+C',
      aliases: ['3d', 'case', 'enclosure', 'housing', 'box', 'lid', 'solid', 'see through', 'hide'],
      keyFrom: here,
      when: in3d,
      run: stepCase,
    },
    ...CASE_MODES.map<Command>((c) => ({
      id: `pcb.3d.case.${c.id}`,
      title: `3D view: Case model: ${c.label.toLowerCase()}`,
      group: 'PCB',
      aliases: ['3d', 'case', 'enclosure', 'housing', 'box', 'lid', c.id === 'see-through' ? 'transparent' : c.id],
      when: in3d,
      run: () => setCase(c.id),
    })),
    {
      id: 'pcb.3d.preview',
      title: '3D view: Height preview (stand the parts of unknown height up, in the picture only)',
      group: 'PCB',
      keys: 'H',
      aliases: ['3d', 'preview at', 'unknown height', 'stand up'],
      keyFrom: here,
      when: in3d,
      run: togglePreview,
    },
  ];
}
