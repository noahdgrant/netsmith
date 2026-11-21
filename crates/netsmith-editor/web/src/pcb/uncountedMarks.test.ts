import { describe, expect, it } from 'vitest';
import type { NetsRow, PcbNets, PcbRouteNet, PcbScene } from '../api';
import { netsOf, routesLayer, routesTrackOf, uncountedMarkup } from './uncountedMarks';

const row = (over: Partial<NetsRow>): NetsRow => ({ net: 'P', length: 20, copper: 30, straight: 20, ratio: 1, vias: 0, layerChanges: 0, layers: ['F.Cu'], notCounted: [], ...over });

const nets: PcbNets = {
  board: 'B',
  layoutStamp: 'l1',
  routesStamp: 'r1',
  routed: false,
  nets: [
    row({
      notCounted: [
        { kind: 'second_way', layer: 'F.Cu', length: 10, at: [12, 10], pieces: [{ from: [12, 10], to: [22, 10], width: 0.25 }] },
        { kind: 'stub', layer: 'B.Cu', length: 1.5, at: [5, 5], pieces: [{ from: [5, 5], to: [6.5, 5], width: 0.2 }] },
      ],
    }),
    row({ net: 'A<B', copper: 20 }),
  ],
  pairs: [],
  groups: [],
};

const track = (layer: string, from: [number, number], to: [number, number]) => ({ layer, width: 0.25, from: { x: from[0], y: from[1] }, mid: null, to: { x: to[0], y: to[1] }, lock: false, drawn: true });
const routes = [{ net: 'P', lock: false, tracks: [track('bottom', [12, 10], [22, 10]), track('top', [10, 10], [24, 10])], vias: [] }] as unknown as PcbRouteNet[];

describe('the copper not counted in a net length', () => {
  it('is one group the Layers menu hides, a run for each, drawn over its pieces, marked current or not', () => {
    const markup = uncountedMarkup(nets, true, []);
    expect(markup).toContain('<g class="l-uncounted" data-testid="uncounted-copper" data-count="2" data-current="true">');
    expect(markup).toContain('<g class="uncounted second-way uncounted-front" data-net="P" data-kind="second_way" data-layer="F.Cu" data-length="10">');
    expect(markup).toContain('<g class="uncounted stub uncounted-back" data-net="P" data-kind="stub"');
    expect(markup).toContain('<line class="uncounted-piece" data-net="P" x1="12" y1="10" x2="22" y2="10" stroke-width="0.25">');
    expect(markup).toContain('a second way of P, 10 mm on F.Cu: not counted in its length');
    expect(uncountedMarkup(nets, false, [])).toContain('data-current="false"');
    expect(uncountedMarkup(null, true, [])).toBe('');
    expect(uncountedMarkup({ ...nets, nets: [row({})] }, true, [])).toBe('');
  });

  it('names the track of the routes file a piece lies on, on its layer, so a click picks it', () => {
    expect(routesTrackOf(routes[0], 'F.Cu', [12, 10], [22, 10])).toBe(2);
    expect(routesTrackOf(routes[0], 'B.Cu', [12, 10], [22, 10])).toBe(1);
    expect(routesTrackOf(routes[0], 'F.Cu', [12, 11], [22, 11])).toBeNull();
    expect(routesTrackOf(undefined, 'F.Cu', [12, 10], [22, 10])).toBeNull();
    expect(uncountedMarkup(nets, true, routes)).toContain('data-route-net="P" data-route-track="2" x1="12"');
    expect([routesLayer('F.Cu'), routesLayer('B.Cu'), routesLayer('In2.Cu')]).toEqual(['top', 'bottom', 'inner2']);
  });

  it('is of the placement drawn only when the board, the stamps and the routing agree', () => {
    const scene = { board: 'B', layoutStamp: 'l1', routesStamp: 'r1', routed: false } as PcbScene;
    expect(netsOf(nets, scene)).toBe(true);
    expect(netsOf(nets, { ...scene, layoutStamp: 'l2' })).toBe(false);
    expect(netsOf(nets, { ...scene, routed: true })).toBe(false);
    expect(netsOf(null, scene)).toBe(false);
  });
});
