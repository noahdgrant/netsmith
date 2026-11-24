import { describe, expect, it } from 'vitest';
import type { PcbFill, PcbScene } from '../api';
import { fillMarkup, fillOf, islandsOf, starvedOf } from './fillMarks';

const counts = { noSpokes: 0, oneSpoke: 1, twoOrMoreSpokes: 0, solid: 0, none: 0, starved: 1, unjoinable: 1 };
const fill: PcbFill = {
  board: 'B',
  layoutStamp: 'l1',
  routesStamp: 'r1',
  routed: false,
  outlineAt: { x: 20, y: 20 },
  fills: [
    {
      layer: 'bottom',
      layerName: 'B.Cu',
      net: 'GND',
      source: 'pour',
      held: false,
      area: 200,
      pads: [],
      counts,
      cleared: [],
      islands: [{ at: { x: 1, y: 1 }, area: 0.5, outline: [] }],
      removedIslands: [
        {
          at: { x: 22, y: 27 },
          area: 0.4821,
          outline: [
            [{ x: 22, y: 27 }, { x: 23, y: 27 }, { x: 23, y: 28.5 }],
            [{ x: 22.5, y: 27.2 }, { x: 22.6, y: 27.2 }],
          ],
        },
        { at: { x: 30, y: 30 }, area: 0.1, outline: [] },
      ],
    },
  ],
  counts,
  starved: [{ layer: 'bottom', net: 'GND', part: 'J1', pad: '2', at: { x: 23.5, y: 28.54 }, spokes: 4, joinedSpokes: 1, tracks: 0 }],
  unjoinable: [{ layer: 'bottom', net: 'GND', part: 'J1', pad: '4', at: { x: 23.5, y: 33.62 }, spokes: 4, why: 'the clearance of track [VCC] (0.2 mm) at the end of 4 spokes' }],
  regulatorGrounds: [],
};

describe('the marks of the fill', () => {
  it('outline each removed island with a shape, not the kept ones, and leave a ring of fewer than three points out', () => {
    const islands = islandsOf(fill);
    expect(islands).toHaveLength(1);
    expect(islands[0]).toMatchObject({ layer: 'B.Cu', side: 'back', net: 'GND', d: 'M22 27L23 27L23 28.5Z' });
  });

  it('ring each starved pad on its layer, with how it is joined, and each pad joined by nothing, with why', () => {
    expect(starvedOf(fill)).toEqual([
      { kind: 'starved', layer: 'B.Cu', side: 'back', net: 'GND', pad: 'J1.2', x: 23.5, y: 28.54, how: '1 of 4 spokes, 0 tracks' },
      { kind: 'unjoinable', layer: 'B.Cu', side: 'back', net: 'GND', pad: 'J1.4', x: 23.5, y: 33.62, how: 'the clearance of track [VCC] (0.2 mm) at the end of 4 spokes' },
    ]);
  });

  it('are two groups the Layers menu hides apart, marked current or not', () => {
    const markup = fillMarkup(fill, true);
    expect(markup).toContain('<g class="l-islands" data-testid="removed-islands" data-current="true">');
    expect(markup).toContain('<path class="removed-island island-back" data-layer="B.Cu" data-net="GND" data-area="0.482" d="M22 27L23 27L23 28.5Z">');
    expect(markup).toContain('<g class="l-starved" data-testid="starved-pads" data-current="true">');
    expect(markup).toContain('data-kind="starved" data-layer="B.Cu" data-net="GND" data-pad="J1.2"><title>J1.2: the GND pour starves it (1 of 4 spokes, 0 tracks; it needs 2)</title>');
    expect(markup).toContain('data-kind="unjoinable" data-layer="B.Cu" data-net="GND" data-pad="J1.4"><title>J1.4: no spoke of the GND pour joins it (the clearance of track [VCC] (0.2 mm) at the end of 4 spokes)</title>');
    expect(fillMarkup({ ...fill, starved: [] }, true)).toContain('data-testid="starved-pads"');
    expect(markup).toContain('<circle class="starved-ring" cx="23.5" cy="28.54"');
    expect(fillMarkup(fill, false)).toContain('data-current="false"');
    expect(fillMarkup(null, true)).toBe('');
    expect(fillMarkup({ ...fill, fills: [], starved: [], unjoinable: [] }, true)).toBe('');
  });

  it('are of the placement drawn only when the board, the stamps and the routing agree', () => {
    const scene = { board: 'B', layoutStamp: 'l1', routesStamp: 'r1', routed: false } as PcbScene;
    expect(fillOf(fill, scene)).toBe(true);
    expect(fillOf(fill, { ...scene, layoutStamp: 'l2' })).toBe(false);
    expect(fillOf(fill, { ...scene, routed: true })).toBe(false);
    expect(fillOf(null, scene)).toBe(false);
  });
});
