import { describe, expect, it } from 'vitest';

import { coverageLine, heightLine, limitTip, partTip, provenanceCounts, readInfo, systemLine, unknownParts, type MemberInfo, type PartInfo3d, type SystemInfo } from './model';

const part = (path: string, refdes: string, provenance: 'stated' | 'catalog' | 'standard' | 'unknown', heightMm: number | null): PartInfo3d => ({
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
        layers: [
          { name: 'F.Mask', kind: 'mask', thickness_mm: 0.01, z_top_mm: 0, z_bottom_mm: -0.01 },
          { name: 'silk', kind: 'ink', thickness_mm: 0, z_top_mm: 0, z_bottom_mm: 0 },
        ],
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
      // A kind it does not know is left out.
      layers: [{ name: 'F.Mask', kind: 'mask', thicknessMm: 0.01, zTopMm: 0, zBottomMm: -0.01 }],
      // A file before the package standard's heights has no `standard` count: none.
      coverage: { known: 2, total: 3, standard: 0 },
      findings: [{ code: 'too_tall', message: 'J1 stands 9 mm tall', parts: ['tall'], limit: 'lid', face: 'top' }],
    });
  });
  it('reads the STEP model a part is drawn from, and the tooltip names its file (ADR 0108)', () => {
    const j = readInfo({ netsmith: { kind: 'part', path: 'usb', refdes: 'J1', side: 'top', provenance: 'stated', height_mm: 3.2, model: { file: 'models/usb.step', sha256: 'ab12', top_mm: 3.16 } } }) as PartInfo3d;
    expect(j.model).toEqual({ file: 'models/usb.step', sha256: 'ab12', topMm: 3.16 });
    expect(partTip(j, []).lines).toEqual(['3.2 mm tall, stated in the design', 'On the top face', 'Drawn from models/usb.step']);
    // A file before models, or a part with none, has none.
    expect((readInfo({ netsmith: { kind: 'part', path: 'r1', model: null } }) as PartInfo3d).model).toBeUndefined();
    // A part of unknown height drawn from a model is not drawn flat, and its height is still not guessed.
    const u = readInfo({ netsmith: { kind: 'part', path: 'u', refdes: 'U1', provenance: 'unknown', height_mm: null, model: { file: 'm.step', sha256: '', top_mm: 1 } } }) as PartInfo3d;
    expect(partTip(u, []).lines[0]).toBe('Height not known, not guessed: the model is a drawing');
  });
  it("reads a package standard's height and its count, apart from the others (ADR 0106)", () => {
    const board = readInfo({ netsmith: { kind: 'board', coverage: { known: 3, total: 4, standard: 1 } } });
    expect(board).toMatchObject({ coverage: { known: 3, total: 4, standard: 1 } });
    const u = readInfo({ netsmith: { kind: 'part', path: 'u1', refdes: 'U1', side: 'top', provenance: 'standard', standard: 'JEDEC MS-012, drawing TI D0008A', height_mm: 1.75 } });
    expect(u).toMatchObject({ provenance: 'standard', standard: 'JEDEC MS-012, drawing TI D0008A', heightMm: 1.75 });
    expect(heightLine(u as PartInfo3d)).toBe("1.75 mm tall, the package standard's maximum (JEDEC MS-012)");
    // A word the page does not know is not a height it can name.
    expect(readInfo({ netsmith: { kind: 'part', path: 'x', provenance: 'nominal', height_mm: 1 } })).toMatchObject({ provenance: 'unknown' });
    expect(coverageLine({ known: 3, total: 4, standard: 1 })).toBe('3 of 4 parts have a known height (1 from the package standard)');
    expect(coverageLine({ known: 2, total: 2, standard: 2 })).toBe('All 2 parts have a known height (2 from the package standard)');
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
    const parts = [part('a', 'J10', 'unknown', null), part('b', 'J2', 'unknown', null), part('c', 'R1', 'catalog', 0.5), part('d', 'J1', 'stated', 9), part('e', 'U1', 'standard', 1.75)];
    expect(unknownParts(parts).map((p) => p.refdes)).toEqual(['J2', 'J10']);
    expect(provenanceCounts(parts)).toEqual({ stated: 1, catalog: 1, standard: 1, unknown: 2 });
  });
});

describe('readInfo: a system scene (ADR 0110)', () => {
  it('names a part of a member by `member/path`, so two members do not share an id', () => {
    const info = readInfo({ netsmith: { kind: 'part', member: 'led', path: 'power_in', refdes: 'J1', side: 'top', provenance: 'catalog', height_mm: 8.65 } }) as PartInfo3d;
    expect(info.path).toBe('led/power_in');
    expect(info.member).toBe('led');
    const alone = readInfo({ netsmith: { kind: 'part', path: 'power_in', refdes: 'J1', side: 'top', provenance: 'catalog', height_mm: 8.65 } }) as PartInfo3d;
    expect(alone.path).toBe('power_in');
    expect(alone.member).toBeUndefined();
  });

  it('reads the system root and its members, and says them in one line', () => {
    const system = readInfo({
      netsmith: {
        kind: 'system',
        system: 'TwoBoards',
        base: 'power',
        mates: [{ a: 'power.power_out', b: 'led.power_in', mated_height_mm: { min: 10.65, max: 11.35 } }],
        findings: [{ code: 'system_collision', severity: 'error', message: 'J2 and the board `led`', parts: ['power/power_out', 'led/'], members: ['power', 'led'] }],
      },
    }) as SystemInfo;
    expect(system.mates[0]).toEqual({ a: 'power.power_out', b: 'led.power_in', minMm: 10.65, maxMm: 11.35 });
    expect(system.findings[0].severity).toBe('error');
    const members = [
      readInfo({ netsmith: { kind: 'member', member: 'power', board: 'TwoBoards/power', placed: true, apart: null } }),
      readInfo({ netsmith: { kind: 'member', member: 'led', board: 'TwoBoards/led', placed: true, apart: null } }),
    ] as MemberInfo[];
    expect(systemLine(system, members)).toBe('2 boards, mated at 10.65–11.35 mm');
    const apart = { ...members[1], placed: false, apart: 'no mated height' };
    expect(systemLine({ ...system, mates: [{ ...system.mates[0], minMm: null, maxMm: null }] }, [members[0], apart])).toBe('2 boards, 1 apart');
  });
});

describe('a case model in the scene file (ADR 0109)', () => {
  it('reads a case, its solids and the fit findings of the root', () => {
    expect(readInfo({ netsmith: { kind: 'case', name: 'lid', file: 'case_fit/lid.step', sha256: 'ab', triangles: 12 } })).toEqual({ kind: 'case', name: 'lid', file: 'case_fit/lid.step', sha256: 'ab' });
    expect(readInfo({ netsmith: { kind: 'case_solid', case: 'base', solid: 'wall_right', closed: false, triangles: 12 } })).toEqual({ kind: 'case_solid', case: 'base', solid: 'wall_right', closed: false });
    const board = readInfo({
      netsmith: {
        kind: 'board',
        findings: [{ code: 'case_collision', message: 'm', parts: ['tall'], severity: 'error', case: 'lid', solid: 'case:lid:lid', at_mm: [16.74, 9.91, 8] }],
      },
    });
    expect(board && 'findings' in board && board.findings[0]).toEqual({ code: 'case_collision', message: 'm', parts: ['tall'], severity: 'error', case: 'lid', solid: 'case:lid:lid', atMm: [16.74, 9.91, 8] });
  });
  it('the tooltip of a part says what the case found of it', () => {
    const findings = [
      { code: 'case_collision', message: '', parts: ['tall'], case: 'lid' },
      { code: 'case_unchecked', message: '', parts: ['bare'], case: 'lid' },
    ];
    expect(partTip(part('tall', 'J1', 'stated', 9), findings).lines).toContain('In the case lid: they collide');
    expect(partTip(part('bare', 'J3', 'unknown', null), findings).lines).toContain('Under the case, not checked');
  });
});
