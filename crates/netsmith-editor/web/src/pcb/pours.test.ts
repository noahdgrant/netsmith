import { describe, expect, it } from 'vitest';
import type { PcbPours, PcbScene } from '../api';
import { POUR_HIDDEN, fills, poursMarkup, shapesOf, sideOf } from './pours';

const pours: PcbPours = {
  board: 'B',
  layoutStamp: 'l1',
  routesStamp: 'r1',
  routed: false,
  error: null,
  layers: [
    { layer: 'F.Cu', nets: [{ net: 'GND', d: 'M0 0L4 0L4 4L0 4ZM1 1L1 2L2 2L2 1Z' }] },
    { layer: 'In2.Cu', nets: [{ net: '3V3', d: 'M0 0L9 0L9 9Z' }] },
    { layer: 'In1.Cu', nets: [{ net: 'GND', d: 'M0 0L8 0L8 8Z' }] },
    { layer: 'B.Cu', nets: [{ net: 'a "b" <c> & d', d: 'M0 0L7 0L7 7Z' }, { net: null, d: 'M1 1L2 1L2 2Z' }] },
  ],
};

const scene = { board: 'B', layoutStamp: 'l1', routesStamp: 'r1', routed: false } as PcbScene;

describe('the fill under the copper', () => {
  it('is drawn deepest layer first, so the layer nearer the viewer is over: inner layers, the back, the front', () => {
    expect(shapesOf(pours).map((s) => s.layer)).toEqual(['In1.Cu', 'In2.Cu', 'B.Cu', 'B.Cu', 'F.Cu']);
  });

  it('gives an inner layer its tone and no other layer one', () => {
    const tone = Object.fromEntries(shapesOf(pours).map((s) => [s.layer, s.tone]));
    expect(tone).toEqual({ 'In1.Cu': 1, 'In2.Cu': 2, 'B.Cu': 0, 'F.Cu': 0 });
    expect(sideOf('F.Cu')).toBe('front');
    expect(sideOf('B.Cu')).toBe('back');
    expect(sideOf('In3.Cu')).toBe('inner');
  });

  it('is a group of paths that stand for their layer, net and side, with the names escaped', () => {
    const markup = poursMarkup(pours, true);
    expect(markup.startsWith('<g class="l-pours"')).toBe(true);
    expect(markup).toContain('<path class="pour pour-front" data-layer="F.Cu" data-net="GND" d="M0 0L4 0L4 4L0 4ZM1 1L1 2L2 2L2 1Z"/>');
    expect(markup).toContain('<path class="pour pour-inner" data-layer="In2.Cu" data-plane="2" data-net="3V3" d="M0 0L9 0L9 9Z"/>');
    expect(markup).toContain('data-net="a &quot;b&quot; &lt;c&gt; &amp; d"');
    // A fill of no net has no net to light.
    expect(markup).toContain('<path class="pour pour-back" data-layer="B.Cu" d="M1 1L2 1L2 2Z"/>');
  });

  it('is nothing when there is no fill, or an empty one', () => {
    expect(poursMarkup(null, true)).toBe('');
    expect(poursMarkup({ ...pours, layers: [] }, true)).toBe('');
    expect(poursMarkup({ ...pours, layers: [{ layer: 'F.Cu', nets: [{ net: 'GND', d: '' }] }] }, true)).toBe('');
  });

  it('says whether it is the fill of the placement drawn', () => {
    expect(poursMarkup(pours, true)).toContain('data-current="true"');
    expect(poursMarkup(pours, false)).toContain('data-current="false"');
  });

  it('follows the layer switch of its copper: the outer layers their copper, the planes Planes', () => {
    expect(POUR_HIDDEN['front-copper']).toBe('.pour-front');
    expect(POUR_HIDDEN['back-copper']).toBe('.pour-back');
    expect(POUR_HIDDEN.planes).toBe('.pour-inner');
    // Every switch that hides one is a switch of the Layers menu, and a pour has exactly one.
    for (const s of shapesOf(pours)) {
      expect(Object.values(POUR_HIDDEN).filter((sel) => sel === `.pour-${s.side}`)).toHaveLength(1);
    }
  });

  it('is drawn only for the placement it was filled from', () => {
    expect(fills(pours, scene)).toBe(true);
    expect(fills(null, scene)).toBe(false);
    expect(fills(pours, null)).toBe(false);
    expect(fills(pours, { ...scene, board: 'C' })).toBe(false);
    expect(fills(pours, { ...scene, layoutStamp: 'l2' })).toBe(false);
    expect(fills(pours, { ...scene, routesStamp: 'r2' })).toBe(false);
    expect(fills(pours, { ...scene, routed: true })).toBe(false);
  });
});
