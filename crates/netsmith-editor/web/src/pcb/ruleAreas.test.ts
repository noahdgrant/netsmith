import { describe, expect, it } from 'vitest';
import type { PcbRuleAreas, PcbScene } from '../api';
import { ruleAreaSaid, ruleAreasMarkup, ruleAreasOf } from './ruleAreas';

const areas: PcbRuleAreas = {
  board: 'B',
  layoutStamp: 'l1',
  routesStamp: 'r1',
  routed: false,
  areas: [
    {
      name: 'antenna',
      part: 'U1',
      layers: ['F.Cu', 'B.Cu'],
      outlines: [[{ x: 1, y: 2 }, { x: 4.0004, y: 2 }, { x: 4, y: 5 }]],
      holes: [[{ x: 2, y: 3 }, { x: 3, y: 3 }, { x: 3, y: 4 }]],
      forbids: ['tracks', 'vias'],
    },
    { name: 'A<B', part: null, layers: ['B.Cu'], outlines: [[{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 1, y: 1 }]], holes: [], forbids: ['copperpour'] },
    { name: 'flat', part: null, layers: ['F.Cu'], outlines: [[{ x: 0, y: 0 }, { x: 1, y: 0 }]], holes: [], forbids: ['tracks'] },
  ],
};

describe('the rule areas', () => {
  it('are one group the Layers menu hides, a hatched path for each area with its holes, its tip naming what it forbids', () => {
    const markup = ruleAreasMarkup(areas, true);
    expect(markup).toContain('<g class="l-rule-areas" data-testid="rule-areas" data-count="2" data-current="true">');
    expect(markup).toContain(
      '<path class="rule-area area-both" data-rule-area="antenna" data-part="U1" data-layers="F.Cu B.Cu" data-forbids="tracks vias" d="M1 2L4 2L4 5ZM2 3L3 3L3 4Z">',
    );
    expect(markup).toContain('<title>antenna of U1 forbids tracks, vias on F.Cu, B.Cu</title>');
    expect(markup).toContain('class="rule-area area-back" data-rule-area="A&lt;B" data-layers');
    expect(markup).toContain('A&lt;B of the board forbids copperpour on B.Cu');
    expect(markup).not.toContain('flat');
    expect(ruleAreasMarkup(areas, false)).toContain('data-current="false"');
    expect(ruleAreasMarkup(null, true)).toBe('');
    expect(ruleAreasMarkup({ ...areas, areas: [] }, true)).toBe('');
  });

  it('are said as view pcb prints them', () => {
    expect(ruleAreaSaid({ name: '', part: null, layers: ['F.Cu'], outlines: [], holes: [], forbids: [] })).toBe(
      'rule area of the board forbids nothing on F.Cu',
    );
  });

  it('are of the placement drawn only when the board, the stamps and the routing agree', () => {
    const scene = { board: 'B', layoutStamp: 'l1', routesStamp: 'r1', routed: false } as PcbScene;
    expect(ruleAreasOf(areas, scene)).toBe(true);
    expect(ruleAreasOf(areas, { ...scene, layoutStamp: 'l2' })).toBe(false);
    expect(ruleAreasOf(areas, { ...scene, routesStamp: 'r2' })).toBe(false);
    expect(ruleAreasOf(areas, { ...scene, routed: true })).toBe(false);
    expect(ruleAreasOf(areas, { ...scene, board: 'C' })).toBe(false);
    expect(ruleAreasOf(null, scene)).toBe(false);
    expect(ruleAreasOf(areas, null)).toBe(false);
  });
});
