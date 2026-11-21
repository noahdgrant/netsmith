import { describe, expect, it } from 'vitest';
import type { PcbScene, PcbStitching } from '../api';
import { stitchingMarkup, stitchingOf } from './stitchMarks';

const stitching: PcbStitching = {
  board: 'B',
  layoutStamp: 'l1',
  routesStamp: 'r1',
  routed: false,
  vias: [
    { net: 'GND', x: 21.25, y: 22.0004, diameter: 0.6 },
    { net: 'A<B', x: 24, y: 22, diameter: 0.5 },
  ],
};

describe('the stitching vias', () => {
  it('are one group the Layers menu hides, a disc for each via where it is, marked current or not', () => {
    const markup = stitchingMarkup(stitching, true);
    expect(markup).toContain('<g class="l-stitching" data-testid="stitching-vias" data-count="2" data-current="true">');
    expect(markup).toContain('<circle class="stitching-via" data-net="GND" cx="21.25" cy="22" r="0.3">');
    expect(markup).toContain('data-net="A&lt;B"');
    expect(markup).toContain('the export will lay: not in the routes file');
    expect(stitchingMarkup(stitching, false)).toContain('data-current="false"');
    expect(stitchingMarkup(null, true)).toBe('');
    expect(stitchingMarkup({ ...stitching, vias: [] }, true)).toBe('');
  });

  it('are of the placement drawn only when the board, the stamps and the routing agree', () => {
    const scene = { board: 'B', layoutStamp: 'l1', routesStamp: 'r1', routed: false } as PcbScene;
    expect(stitchingOf(stitching, scene)).toBe(true);
    expect(stitchingOf(stitching, { ...scene, layoutStamp: 'l2' })).toBe(false);
    expect(stitchingOf(stitching, { ...scene, routesStamp: 'r2' })).toBe(false);
    expect(stitchingOf(stitching, { ...scene, routed: true })).toBe(false);
    expect(stitchingOf(stitching, { ...scene, board: 'C' })).toBe(false);
    expect(stitchingOf(null, scene)).toBe(false);
    expect(stitchingOf(stitching, null)).toBe(false);
  });
});
