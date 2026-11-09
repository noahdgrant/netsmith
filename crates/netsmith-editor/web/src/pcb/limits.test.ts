import { describe, expect, it } from 'vitest';

import type { PcbBox, PcbPart } from '../api';
import { heightLines, heldTip, limitMove, tooTallFor, under, zonesOf } from './limits';

const box = (x: number, y: number, width: number, height: number): PcbBox => ({ x, y, width, height });

function part(path: string, body: PcbBox, height: number | null, side: 'top' | 'bottom' = 'top'): PcbPart {
  return {
    refdes: path.toUpperCase(),
    path,
    kind: 'Header',
    value: '',
    package: null,
    x: body.x,
    y: body.y,
    rotation: 0,
    side,
    locked: false,
    placement: 'hand',
    courtyard: body,
    body,
    height,
    pads: [],
  };
}

// A lid over x from 0 to 20 that allows 3 mm on the top face.
const lid = (layers = ['top']) =>
  zonesOf([
    { name: 'lid', origin: 'source', area: box(0, 0, 20, 24), width: 20, height: 24, layers, tracks: false, vias: false, copper: false, parts: false, maxHeight: 3 },
    { name: 'plain', origin: 'source', area: box(30, 0, 5, 5), width: 5, height: 5, layers: ['top'], tracks: true, vias: true, copper: true, parts: true, maxHeight: null },
  ]);

describe('height zones', () => {
  it('are the keepouts that state a limit and have a place', () => {
    expect(lid().map((z) => z.name)).toEqual(['lid']);
  });

  it('limit the face their layers name, and the body that overlaps them', () => {
    const tall = part('tall', box(15, 5, 4, 4), 9);
    expect(under(tall, lid()).map((z) => z.name)).toEqual(['lid']);
    expect(under(tall, lid(['bottom']))).toEqual([]);
    expect(under(part('back', box(15, 5, 4, 4), 9, 'bottom'), lid(['bottom'])).map((z) => z.name)).toEqual(['lid']);
    expect(under(part('away', box(25, 5, 4, 4), 9), lid())).toEqual([]);
  });

  it('are too tall only for a part with a stated height over the limit', () => {
    expect(tooTallFor(part('tall', box(15, 5, 4, 4), 9), lid()).length).toBe(1);
    expect(tooTallFor(part('flat', box(15, 5, 4, 4), 3), lid()).length).toBe(0);
    expect(tooTallFor(part('unknown', box(15, 5, 4, 4), null), lid()).length).toBe(0);
  });
});

describe('the hover about height', () => {
  it('says how tall a part is, and for each limit it stands under what it allows', () => {
    expect(heightLines(part('tall', box(15, 5, 4, 4), 9.5), lid())).toEqual(['stands 9.5 mm tall', 'lid allows 3 mm here: 6.5 mm too tall']);
    expect(heightLines(part('flat', box(15, 5, 4, 4), 2), lid())).toEqual(['stands 2 mm tall', 'lid allows 3 mm here: fits']);
    expect(heightLines(part('unknown', box(15, 5, 4, 4), null), lid())).toEqual(['height not stated', 'lid allows 3 mm here: unchecked']);
    expect(heightLines(part('away', box(25, 5, 4, 4), 9), lid())).toEqual(['stands 9 mm tall']);
  });
});

describe('a drag held by a height limit', () => {
  const tall = part('tall', box(24, 5, 4, 4), 9);

  it('goes where it likes when nothing is in the way', () => {
    const r = limitMove([tall], lid(), { x: 2, y: 3 }, { x: 0, y: 0 }, 0.5);
    expect(r).toEqual({ delta: { x: 2, y: 3 }, held: null });
  });

  it('stops at the edge of a zone the part is too tall for, and says which', () => {
    // Moving 10 mm left would put the body (x 24..28) at 14..18, inside the lid (x up to 20).
    const r = limitMove([tall], lid(), { x: -10, y: 0 }, { x: 0, y: 0 }, 0.5);
    expect(r.delta).toEqual({ x: -4, y: 0 });
    expect(r.held).toEqual({ zone: 'lid', limit: 3, refdes: 'TALL', height: 9 });
    expect(heldTip(r.held!).lines).toEqual(['TALL stands 9 mm tall', 'lid allows 3 mm']);
  });

  it('slides along the edge when the pointer also moves along it', () => {
    const r = limitMove([tall], lid(), { x: -10, y: 6 }, { x: 0, y: 0 }, 0.5);
    expect(r.delta).toEqual({ x: -4, y: 6 });
    expect(r.held?.zone).toBe('lid');
  });

  it('does not jump over a thin zone', () => {
    const thin = zonesOf([{ name: 'rib', origin: 'source', area: box(20, 0, 1, 24), width: 1, height: 24, layers: ['top'], tracks: false, vias: false, copper: false, parts: false, maxHeight: 3 }]);
    const r = limitMove([tall], thin, { x: -12, y: 0 }, { x: 0, y: 0 }, 0.5);
    expect(r.delta.x).toBe(-3);
    expect(r.held?.zone).toBe('rib');
  });

  it('lets a part that is not too tall, or states no height, go under the limit', () => {
    const flat = part('flat', box(24, 5, 4, 4), 2);
    const unknown = part('unknown', box(24, 5, 4, 4), null);
    expect(limitMove([flat], lid(), { x: -10, y: 0 }, { x: 0, y: 0 }, 0.5).held).toBeNull();
    expect(limitMove([unknown], lid(), { x: -10, y: 0 }, { x: 0, y: 0 }, 0.5).held).toBeNull();
  });

  it('holds every part taken, and lets one that starts under a limit it is over move out of it', () => {
    const inside = part('inside', box(5, 5, 4, 4), 9);
    const out = limitMove([inside], lid(), { x: 30, y: 0 }, { x: 0, y: 0 }, 0.5);
    expect(out.held).toBeNull();
    const both = limitMove([tall, part('flat', box(30, 5, 4, 4), 1)], lid(), { x: -12, y: 0 }, { x: 0, y: 0 }, 0.5);
    expect(both.delta.x).toBe(-4);
  });

  it('works without a grid', () => {
    const r = limitMove([tall], lid(), { x: -10, y: 0 }, { x: 0, y: 0 }, 0);
    expect(r.delta.x).toBeCloseTo(-4, 6);
  });
});
