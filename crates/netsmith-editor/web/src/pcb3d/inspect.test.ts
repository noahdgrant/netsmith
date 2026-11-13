import { describe, expect, it } from 'vitest';

import { contrast, parseHex } from '../contrast';
import css from '../themes.css?raw';
import { DIM, KINDS, MARGIN_EASY_MM, extrudeFlat, findingTargets, inspectTokens, kindCounts, kindSwatch, litNets, litReport, marginBand, marginColour, pieceNet, xrayOpacity } from './inspect';
import { COLOUR_BY, cycleColourBy, colourBy, heightPreview, inspectCommands, previewHeight, togglePreview } from './inspectMode';
import type { LimitInfo, PartInfo3d } from './model';
import { LOOKS, type Role } from './palette';

const part = (path: string, refdes: string, over: Partial<PartInfo3d> = {}): PartInfo3d => ({
  kind: 'part',
  path,
  refdes,
  side: 'top',
  provenance: 'stated',
  heightMm: 1,
  partKind: '',
  category: null,
  marginMm: null,
  marginLimit: null,
  ...over,
});

const limit = (name: string, face: 'top' | 'bottom'): LimitInfo => ({ kind: 'height_limit', name, face, maxHeightMm: 3, violated: false, marginMm: 1, tallest: null, unknown: [], under: [] });

describe('colour by kind', () => {
  it('takes the file’s category, and a word it does not know (or none) is other', () => {
    expect(kindSwatch('resistor')).toBe('resistor');
    expect(kindSwatch('connector')).toBe('connector');
    expect(kindSwatch('fuse')).toBe('other');
    expect(kindSwatch('mechanical')).toBe('other');
    expect(kindSwatch(null)).toBe('other');
    expect(kindSwatch('spaceship')).toBe('other');
  });
  it('counts the parts of each kind in the legend’s order, leaving out the kinds with none', () => {
    const parts = [part('a', 'R1', { category: 'resistor' }), part('b', 'J1', { category: 'connector' }), part('c', 'R2', { category: 'resistor' }), part('d', 'X1', { category: 'fuse' })];
    expect(kindCounts(parts)).toEqual([
      { id: 'connector', label: 'Connector', count: 1 },
      { id: 'resistor', label: 'Resistor', count: 2 },
      { id: 'other', label: 'Other', count: 1 },
    ]);
  });
});

describe('the height map', () => {
  const c = { easy: '#00ff00', tight: '#ffaa00', over: '#ff0000', none: '#808080' };
  it('red when too tall, amber blending to green up to the easy margin, grey with no margin', () => {
    expect(marginBand(null)).toBe('none');
    expect(marginBand(-0.01)).toBe('over');
    expect(marginBand(0)).toBe('tight');
    expect(marginBand(MARGIN_EASY_MM)).toBe('easy');
    expect(marginColour(-2, c)).toBe('#ff0000');
    expect(marginColour(null, c)).toBe('#808080');
    expect(marginColour(0, c)).toBe('#ffaa00');
    expect(marginColour(MARGIN_EASY_MM / 2, c)).toBe('#80d500');
    expect(marginColour(5, c)).toBe('#00ff00');
  });
});

describe('x-ray', () => {
  it('makes the board, the mask and the bodies see-through, and leaves the copper, the gold and the unknowns as they are', () => {
    for (const r of ['substrate', 'prepreg', 'mask-top', 'mask-bottom', 'mask-over-copper', 'body', 'body-catalog', 'body-standard', 'metal', 'mark', 'silk', 'case-solid'] as Role[]) {
      const o = xrayOpacity(r);
      expect(o, r).not.toBeNull();
      expect(o!).toBeLessThan(0.5);
    }
    // A case model's solids are see-through already (ADR 0109).
    for (const r of ['copper', 'finish', 'body-unknown', 'limit', 'limit-violated', 'case'] as Role[]) expect(xrayOpacity(r), r).toBeNull();
    // Every role the view draws is one of the two lists above.
    expect(Object.keys(LOOKS).length).toBe(18);
    expect(DIM.opacity).toBeLessThan(1);
  });
});

describe('lit nets', () => {
  it('a picked net in the selection colour wins over the same net hovered', () => {
    expect([...litNets(null, [])]).toEqual([]);
    expect([...litNets('GND', ['VCC', 'GND'])]).toEqual([
      ['VCC', 'hover'],
      ['GND', 'select'],
    ]);
  });
});

describe('a lit net, in a scene with copper (the contract of docs/BOARD3D.md)', () => {
  // A hand-made scene: VIN on both layers with a via and a pad's barrel, GND on the top layer.
  const pieces = [
    { kind: 'copper', layer: 'F.Cu', net: null },
    { kind: 'copper', layer: 'F.Cu', net: 'VIN' },
    { kind: 'copper', layer: 'F.Cu', net: 'GND' },
    { kind: 'copper', layer: 'B.Cu', net: 'VIN' },
    { kind: 'hole', layer: null, net: 'VIN' },
    { kind: 'hole', layer: null, net: 'GND' },
    { kind: 'hole', layer: null, net: null, path: 'power_in', pad: '1' },
    { kind: 'hole', layer: null, net: null, path: 'power_in', pad: '2' },
  ];
  const pads = new Map([
    ['power_in:1', 'VIN'],
    ['power_in:2', 'GND'],
  ]);
  it('lights the net on every layer, its via and its pad’s barrel, and dims the rest', () => {
    expect(litReport(pieces, litNets('VIN', []), pads)).toEqual({ layers: ['B.Cu', 'F.Cu'], barrels: 2, vias: 1, dimmed: 4 });
    expect(litReport(pieces, litNets(null, ['GND']), pads)).toEqual({ layers: ['F.Cu'], barrels: 2, vias: 1, dimmed: 5 });
    expect(litReport(pieces, litNets(null, []), pads)).toEqual({ layers: [], barrels: 0, vias: 0, dimmed: 0 });
  });
  it('a pad’s barrel takes its pad’s net, and copper with no net has none', () => {
    expect(pieceNet(pieces[6], pads)).toBe('VIN');
    expect(pieceNet(pieces[6], new Map())).toBeNull();
    expect(pieceNet(pieces[0], pads)).toBeNull();
  });
});

describe('the height preview', () => {
  it('reads a height a person types, from 0.1 to 50 mm, and nothing else', () => {
    expect(previewHeight('1')).toBe(1);
    expect(previewHeight(' 2,5 mm')).toBe(2.5);
    expect(previewHeight('0')).toBeNull();
    expect(previewHeight('51')).toBeNull();
    expect(previewHeight('tall')).toBeNull();
  });
  it('is off at the start, and its switch keeps the height', () => {
    expect(heightPreview.value).toEqual({ on: false, mm: 1 });
    togglePreview();
    expect(heightPreview.value).toEqual({ on: true, mm: 1 });
    togglePreview();
    expect(heightPreview.value.on).toBe(false);
  });
  it('stands a flat outline up as a closed solid: both ends and a wall on each outside edge', () => {
    // A square of two triangles at y = 0.0001, as the file's flat outline is.
    const y = 0.0001;
    const pos = [0, y, 0, 1, y, 0, 1, y, 1, 0, y, 1];
    const out = extrudeFlat(pos, [0, 1, 2, 0, 2, 3], 0, 2);
    // 2 triangles at each end and 2 per side: 12 triangles of 3 points.
    expect(out.length).toBe(12 * 3 * 3);
    const ys = new Set<number>();
    for (let i = 1; i < out.length; i += 3) ys.add(out[i]);
    expect([...ys].sort()).toEqual([0, 2]);
    // The inside diagonal (0-2) is not a wall: 8 walls' triangles, for the 4 outside edges.
    const walls = (out.length - 4 * 9) / 9;
    expect(walls).toBe(8);
    // Unindexed points are triangles as they come: one triangle, its 2 ends and 3 walls.
    expect(extrudeFlat(pos.slice(0, 9), null, 0, 2).length).toBe((2 + 6) * 9);
  });
});

describe('a finding in 3D', () => {
  it('finds its parts by designator and the limit on their face', () => {
    const parts = [part('tall', 'J1'), part('flat', 'J2'), part('under', 'J9', { side: 'bottom' })];
    const limits = [limit('lid', 'top'), limit('lid', 'bottom'), limit('rib', 'top')];
    expect(findingTargets(['J1'], 'lid', parts, limits)).toEqual({ paths: ['tall'], limits: ['lid:top'] });
    expect(findingTargets(['J1', 'J2'], null, parts, limits)).toEqual({ paths: ['tall', 'flat'], limits: [] });
    // A limit named with no part on it: every face of it.
    expect(findingTargets([], 'lid', parts, limits)).toEqual({ paths: [], limits: ['lid:top', 'lid:bottom'] });
  });
});

describe('the commands', () => {
  it('have keys that are not the view’s others, and cycle the colouring in the menu’s order', () => {
    const cmds = inspectCommands(
      () => true,
      () => true,
    );
    const keys = cmds.map((c) => c.keys).filter(Boolean);
    expect(keys).toEqual(['V', 'C', 'Shift+C', 'H']);
    // Not the 3D view's others, its tools' (measure M, section S and its axis X Y Z, layers apart E), the 2D board's (R F L K) or the schematic's (R M).
    for (const k of keys) expect(['D', 'T', 'B', 'I', 'P', 'F', 'M', 'S', 'X', 'Y', 'Z', 'E', 'R', 'L', 'K', '0']).not.toContain(k);
    expect(colourBy.value).toBe('provenance');
    for (const c of [...COLOUR_BY.slice(1), COLOUR_BY[0]]) {
      cycleColourBy();
      expect(colourBy.value).toBe(c.id);
    }
  });
});

const THEMES = {
  dark: ':root, :root[data-theme="dark"]',
  light: ':root[data-theme="light"]',
  'hc-dark': ':root[data-theme="hc-dark"]',
  'hc-light': ':root[data-theme="hc-light"]',
};

function tokens(selector: string): Record<string, string> {
  const at = css.indexOf(`${selector} {`);
  const body = css.slice(css.indexOf('{', at) + 1, css.indexOf('\n}', at));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}

describe('the tokens of kind, the height map and the preview', () => {
  for (const [theme, selector] of Object.entries(THEMES)) {
    const t = tokens(selector);
    const c = (token: string) => {
      let v = t[token];
      for (let i = 0; i < 5 && v?.startsWith('var('); i++) v = t[v.slice(4, -1)];
      return v;
    };
    it(`${theme}: has them all, each a colour; each kind stands out from the mask, and the kinds are all different`, () => {
      for (const token of inspectTokens()) expect(parseHex(c(token) ?? ''), `${token} in ${theme}`).not.toBeNull();
      for (const k of KINDS) expect(contrast(c(k.token), c('--pcb3d-mask'))!, `${k.token} on the mask in ${theme}`).toBeGreaterThanOrEqual(2.5);
      expect(new Set(KINDS.map((k) => c(k.token))).size).toBe(KINDS.length);
      expect(new Set([c('--pcb3d-margin-easy'), c('--pcb3d-margin-tight'), c('--pcb3d-margin-over'), c('--pcb3d-margin-none')]).size).toBe(4);
      expect(contrast(c('--pcb3d-preview'), c('--pcb3d-mask'))!, `the preview on the mask in ${theme}`).toBeGreaterThanOrEqual(3);
      expect(parseHex(c('--pcb3d-finish') ?? '')).not.toBeNull();
      expect(parseHex(c('--pcb3d-mask-copper') ?? '')).not.toBeNull();
    });
  }
});
