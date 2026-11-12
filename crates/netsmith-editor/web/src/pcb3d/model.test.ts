import { describe, expect, it } from 'vitest';

import { coverageLine, heightLine, limitTip, partTip, provenanceCounts, readInfo, unknownParts, type PartInfo3d } from './model';

const part = (path: string, refdes: string, provenance: 'stated' | 'catalog' | 'unknown', heightMm: number | null): PartInfo3d => ({
  kind: 'part',
  path,
  refdes,
  side: 'top',
  provenance,
  heightMm,
  partKind: '',
  category: null,
  marginMm: null,
  marginLimit: null,
});

describe('readInfo: the extras of the scene file (schema 1)', () => {
  it('reads the board: its size, its coverage and its findings', () => {
    const info = readInfo({
      netsmith: {
        kind: 'board',
        schema: 1,
        board: 'Lidded',
        thickness_mm: 1.6,
        width_mm: 40,
        height_mm: 24,
        coverage: { known: 2, total: 3 },
        findings: [{ code: 'too_tall', message: 'J1 stands 9 mm tall', parts: ['tall'], limit: 'lid', face: 'top' }],
      },
    });
    expect(info).toEqual({
      kind: 'board',
      schema: 1,
      board: 'Lidded',
      widthMm: 40,
      heightMm: 24,
      thicknessMm: 1.6,
      coverage: { known: 2, total: 3 },
      findings: [{ code: 'too_tall', message: 'J1 stands 9 mm tall', parts: ['tall'], limit: 'lid', face: 'top' }],
    });
  });
  it('reads a part, and an unknown height stays null: never zero', () => {
    expect(readInfo({ netsmith: { kind: 'part', path: 'tall', refdes: 'J1', side: 'top', provenance: 'stated', height_mm: 9, z_bottom_mm: 0, z_top_mm: 9 } })).toEqual(part('tall', 'J1', 'stated', 9));
    const bare = readInfo({ netsmith: { kind: 'part', path: 'bare', refdes: 'J3', side: 'bottom', provenance: 'unknown', height_mm: null, z_bottom_mm: null, z_top_mm: -1.6 } });
    expect(bare).toEqual({ ...part('bare', 'J3', 'unknown', null), side: 'bottom' });
  });
  it('reads the kind of a part and its margin, where the file has them', () => {
    const r = readInfo({ netsmith: { kind: 'part', path: 'r', refdes: 'R1', provenance: 'stated', height_mm: 0.5, part_kind: 'Resistor', category: 'resistor', margin_mm: 2.5, margin_limit: 'lid' } });
    expect(r).toMatchObject({ partKind: 'Resistor', category: 'resistor', marginMm: 2.5, marginLimit: 'lid' });
    expect(partTip(r as PartInfo3d, []).lines).toEqual(['0.5 mm tall, stated in the design', 'On the top face · Resistor', '2.5 mm under the limit lid']);
  });
  it('reads a net of copper and a via', () => {
    expect(readInfo({ netsmith: { kind: 'copper', layer: 'F.Cu', net: 'GND' } })).toEqual({ kind: 'copper', layer: 'F.Cu', net: 'GND' });
    expect(readInfo({ netsmith: { kind: 'copper', layer: 'F.Cu', net: null } })).toEqual({ kind: 'copper', layer: 'F.Cu', net: null });
    expect(readInfo({ netsmith: { kind: 'hole', net: 'VCC' } })).toEqual({ kind: 'hole', layer: null, net: 'VCC' });
    expect(readInfo({ netsmith: { kind: 'hole', path: 'power_in', refdes: 'J1', pad: '1' } })).toEqual({ kind: 'hole', layer: null, net: null, path: 'power_in', pad: '1' });
  });
  it('a provenance it does not know is unknown, not a height source', () => {
    expect((readInfo({ netsmith: { kind: 'part', path: 'a', refdes: 'R1', provenance: 'nominal', height_mm: 1 } }) as PartInfo3d).provenance).toBe('unknown');
  });
  it('reads a height limit', () => {
    expect(readInfo({ netsmith: { kind: 'height_limit', name: 'lid', face: 'top', max_height_mm: 3, tallest: 'tall', margin_mm: -6, violated: true, unknown: ['bare'] } })).toEqual({
      kind: 'height_limit',
      name: 'lid',
      face: 'top',
      maxHeightMm: 3,
      violated: true,
      marginMm: -6,
      tallest: 'tall',
      unknown: ['bare'],
      under: [],
    });
  });
  it('keeps the kind of a node it does not know (later shapes) and nothing else', () => {
    expect(readInfo({ netsmith: { kind: 'silk', face: 'top' } })).toEqual({ kind: 'silk' });
  });
  it('a node with no extras of its own is null', () => {
    expect(readInfo({})).toBeNull();
    expect(readInfo(null)).toBeNull();
    expect(readInfo({ netsmith: { kind: 3 } })).toBeNull();
  });
});

describe('the words of the view', () => {
  it('the coverage chip counts the parts with a known height', () => {
    expect(coverageLine({ known: 2, total: 3 })).toBe('2 of 3 parts have a known height');
    expect(coverageLine({ known: 1, total: 3 })).toBe('1 of 3 parts has a known height');
    expect(coverageLine({ known: 4, total: 4 })).toBe('All 4 parts have a known height');
    expect(coverageLine({ known: 1, total: 1 })).toBe('The part has a known height');
    expect(coverageLine({ known: 0, total: 0 })).toBe('No parts with a body');
  });
  it('a height says where it came from; an unknown one says it is not guessed', () => {
    expect(heightLine({ heightMm: 9, provenance: 'stated' })).toBe('9 mm tall, stated in the design');
    expect(heightLine({ heightMm: 1.234, provenance: 'catalog' })).toBe('1.23 mm tall, from the catalog row');
    expect(heightLine({ heightMm: null, provenance: 'unknown' })).toBe('Height not known: drawn flat and hatched, not guessed');
  });
  it('the tooltip of a part names it, its height, its face and the findings about it', () => {
    const findings = [
      { code: 'too_tall', message: '', parts: ['tall'], limit: 'lid' },
      { code: 'height_unchecked', message: '', parts: ['bare'], limit: 'lid' },
      { code: 'body_overlap', message: '', parts: ['tall', 'flat'] },
    ];
    expect(partTip(part('tall', 'J1', 'stated', 9), findings, 'Conn')).toEqual({
      title: 'J1 Conn',
      lines: ['9 mm tall, stated in the design', 'On the top face', 'Too tall for the limit lid', 'Its body overlaps another'],
    });
    expect(partTip(part('bare', 'J3', 'unknown', null), findings).lines).toContain('Under the limit lid, which is not checked over it');
  });
  it('the tooltip of a limit says what it allows and how it stands', () => {
    const l = { kind: 'height_limit' as const, name: 'lid', face: 'top' as const, maxHeightMm: 3, violated: true, marginMm: -6, tallest: 'tall', unknown: ['bare'], under: ['tall', 'bare'] };
    expect(limitTip(l)).toEqual({ title: 'Height limit lid', lines: ['Parts at most 3 mm tall on the top face', 'A part is 6 mm too tall', '1 part of unknown height under it, not checked'] });
    expect(limitTip({ ...l, violated: false, marginMm: 1.5, unknown: [] }).lines).toEqual(['Parts at most 3 mm tall on the top face', '1.5 mm to spare over the tallest part']);
  });
  it('lists the parts of unknown height by designator, and counts each source', () => {
    const parts = [part('a', 'J10', 'unknown', null), part('b', 'J2', 'unknown', null), part('c', 'R1', 'catalog', 0.5), part('d', 'J1', 'stated', 9)];
    expect(unknownParts(parts).map((p) => p.refdes)).toEqual(['J2', 'J10']);
    expect(provenanceCounts(parts)).toEqual({ stated: 1, catalog: 1, unknown: 2 });
  });
});
