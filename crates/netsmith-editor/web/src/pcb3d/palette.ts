// How the 3D view colours the scene's materials. The `.glb` file names its materials (ADR 0105,
// docs/BOARD3D.md) and carries the rasterizer's light colours; the page draws each name in the
// theme's own colour instead, from the `--pcb3d-*` tokens of themes.css, so the view follows the
// four themes and changes with them. A part's body takes its tone from where its height came from
// (stated, catalog), and a part of unknown height is a hatched outline. Nothing here touches
// three.js: the view reads the table and the tokens, and a test holds the tokens to their contrast.

/** What the view draws: a material of the file, or a body by its height's provenance. */
export type Role =
  | 'substrate'
  | 'mask-top'
  | 'mask-bottom'
  | 'copper'
  | 'body'
  | 'body-catalog'
  | 'body-unknown'
  | 'metal'
  | 'mark'
  | 'limit'
  | 'limit-violated'
  | 'finish'
  | 'mask-over-copper';

/** The material names of the file's contract (schema 1), in its order: the ten of the first file, then the copper's two (the gold where the mask is open, the mask over copper). */
export const FILE_MATERIALS = ['substrate', 'mask-top', 'mask-bottom', 'copper', 'body', 'body-unknown', 'metal', 'mark', 'limit', 'limit-violated', 'finish', 'mask-over-copper'] as const;

export interface Look {
  /** The theme token of its colour. */
  token: string;
  metalness: number;
  roughness: number;
  /** Below 1, see-through (a limit's volume). */
  opacity: number;
  /** Drawn with the hatch (an unknown height). */
  hatch?: boolean;
}

export const LOOKS: Record<Role, Look> = {
  substrate: { token: '--pcb3d-substrate', metalness: 0, roughness: 0.85, opacity: 1 },
  'mask-top': { token: '--pcb3d-mask', metalness: 0, roughness: 0.55, opacity: 1 },
  'mask-bottom': { token: '--pcb3d-mask', metalness: 0, roughness: 0.55, opacity: 1 },
  copper: { token: '--pcb3d-copper', metalness: 0.6, roughness: 0.4, opacity: 1 },
  body: { token: '--pcb3d-body', metalness: 0, roughness: 0.6, opacity: 1 },
  'body-catalog': { token: '--pcb3d-body-catalog', metalness: 0, roughness: 0.6, opacity: 1 },
  'body-unknown': { token: '--pcb3d-unknown', metalness: 0, roughness: 0.9, opacity: 0.9, hatch: true },
  metal: { token: '--pcb3d-metal', metalness: 0.7, roughness: 0.35, opacity: 1 },
  mark: { token: '--pcb3d-mark', metalness: 0, roughness: 0.8, opacity: 1 },
  limit: { token: '--pcb3d-limit', metalness: 0, roughness: 1, opacity: 0.14 },
  'limit-violated': { token: '--pcb3d-limit-violated', metalness: 0, roughness: 1, opacity: 0.24 },
  finish: { token: '--pcb3d-finish', metalness: 0.75, roughness: 0.3, opacity: 1 },
  'mask-over-copper': { token: '--pcb3d-mask-copper', metalness: 0, roughness: 0.5, opacity: 1 },
};

/** The tokens the view reads besides the materials'. */
export const TOKENS = {
  background: '--pcb3d-bg',
  edge: '--pcb3d-edge',
  select: '--sel',
  hover: '--hover-lit',
} as const;

export type Provenance = 'stated' | 'catalog' | 'unknown';

/** The role a mesh is drawn in, from its material's name and, for a part's body, where the part's
 * height came from. A name the page does not know (a later file's) is null: it keeps the file's colour. */
export function roleOf(material: string, provenance: Provenance | null): Role | null {
  if (material === 'body') return provenance === 'catalog' ? 'body-catalog' : provenance === 'unknown' ? 'body-unknown' : 'body';
  return (FILE_MATERIALS as readonly string[]).includes(material) ? (material as Role) : null;
}

/** Every token the view reads, for the test that each theme has them. */
export function allTokens(): string[] {
  return [...new Set([...Object.values(LOOKS).map((l) => l.token), ...Object.values(TOKENS)])];
}

type Rgb = [number, number, number];

const luma = ([r, g, b]: Rgb) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;

/** A surface lit by the hover: its own tone moved toward the theme's hover ink, so it is lit and
 * not picked. Where the ink is close to the tone (a dark body under the light theme's dark ink),
 * it moves the other way, toward white or black, so the light always shows. */
export function lit(base: Rgb, ink: Rgb, amount = 0.4): Rgb {
  const toward: Rgb = Math.abs(luma(base) - luma(ink)) >= 0.25 ? ink : luma(ink) < 0.5 ? [255, 255, 255] : [0, 0, 0];
  return [0, 1, 2].map((i) => Math.round(base[i] + (toward[i] - base[i]) * amount)) as Rgb;
}
