import { describe, expect, it } from 'vitest';

import type { LimitInfo, PartInfo3d, StackLayer } from './model';
import {
  bestSnap,
  defaultCut,
  delta,
  explodeGap,
  explodeSlots,
  faceSlot,
  fixed,
  keepBelow,
  measureWords,
  nearestView,
  parseCut,
  partReadout,
  renderCommand,
  shellWord,
  slabRole,
  type Snap,
  spreadMembers,
} from './tools';

const two: StackLayer[] = [
  { name: 'F.Mask', kind: 'mask', thicknessMm: 0.01, zTopMm: 0, zBottomMm: -0.01 },
  { name: 'F.Cu', kind: 'copper', thicknessMm: 0.035, zTopMm: -0.01, zBottomMm: -0.045 },
  { name: 'core', kind: 'core', thicknessMm: 1.51, zTopMm: -0.045, zBottomMm: -1.555 },
  { name: 'B.Cu', kind: 'copper', thicknessMm: 0.035, zTopMm: -1.555, zBottomMm: -1.59 },
  { name: 'B.Mask', kind: 'mask', thicknessMm: 0.01, zTopMm: -1.59, zBottomMm: -1.6 },
];

const snap = (kind: Snap['kind'], label: string = kind): Snap => ({ kind, at: [0, 0, 0], label });

describe('the measure tool', () => {
  it('writes a length with two or three decimals and no negative zero', () => {
    expect(fixed(1.6)).toBe('1.60');
    expect(fixed(0.035)).toBe('0.035');
    expect(fixed(12.3456)).toBe('12.346');
    expect(fixed(-0.0001)).toBe('0.00');
    expect(fixed(-2.5)).toBe('-2.50');
  });
  it('gives the distance and its parts from the first point to the second', () => {
    expect(delta([0, 0, 0], [3, 4, 0]).d).toBe(5);
    expect(measureWords([1, 1, 0], [4, 5, -1.6])).toEqual({ distance: '5.25 mm', parts: 'dx 3.00 · dy 4.00 · dz -1.60' });
  });
  it('snaps to a pad before a corner, a corner before an edge, and a face only where the pointer is', () => {
    expect(bestSnap([{ snap: snap('corner'), px: 3 }, { snap: snap('pad'), px: 10 }])?.kind).toBe('pad');
    expect(bestSnap([{ snap: snap('corner'), px: 3 }, { snap: snap('pad'), px: 20 }])?.kind).toBe('corner');
    expect(bestSnap([{ snap: snap('edge'), px: 4 }, { snap: snap('top'), px: 0 }])?.kind).toBe('edge');
    expect(bestSnap([{ snap: snap('edge'), px: 14 }, { snap: snap('face'), px: 0 }])?.kind).toBe('face');
    expect(bestSnap([{ snap: snap('pad', 'far'), px: 8 }, { snap: snap('pad', 'near'), px: 2 }])?.label).toBe('near');
    expect(bestSnap([])).toBeNull();
  });
  describe("a part's height and its margin to the limit above it", () => {
    const part = (heightMm: number | null): PartInfo3d => ({ kind: 'part', path: 'u1', refdes: 'U1', side: 'top', provenance: heightMm === null ? 'unknown' : 'stated', heightMm, partKind: '', category: null, marginMm: null, marginLimit: null });
    const limit = (name: string, maxHeightMm: number, min: [number, number], max: [number, number], face: 'top' | 'bottom' = 'top') => ({
      info: { kind: 'height_limit', name, face, maxHeightMm, violated: false, marginMm: null, tallest: null, unknown: [], under: [] } as LimitInfo,
      min,
      max,
    });
    const plan = { min: [10, 10] as [number, number], max: [14, 12] as [number, number] };
    it('says the height above the face and the least margin of the limits over it', () => {
      const r = partReadout(part(2), plan, [limit('lid', 3, [0, 0], [20, 20]), limit('rib', 2.5, [12, 0], [13, 30]), limit('far', 1, [30, 30], [40, 40]), limit('under', 0.5, [0, 0], [20, 20], 'bottom')]);
      expect(r.height).toBe('2.00 mm above the top face');
      expect(r.marginMm).toBe(0.5);
      expect(r.limit).toBe('0.50 mm to spare under the limit rib (2.50 mm)');
    });
    it('says by how much a part is over its limit', () => {
      expect(partReadout(part(9), plan, [limit('lid', 3, [0, 0], [20, 20])]).limit).toBe('6.00 mm over the limit lid (3.00 mm)');
    });
    it('measures no unknown height and guesses no margin', () => {
      const r = partReadout(part(null), plan, [limit('lid', 3, [0, 0], [20, 20])]);
      expect(r.marginMm).toBeNull();
      expect(r.height).toContain('not known');
      expect(r.limit).toContain('not checked');
    });
    it('says when no limit is above it', () => {
      expect(partReadout(part(1), plan, []).limit).toBe('No height limit above it');
    });
  });
});

describe('the section plane', () => {
  it('keeps the side away from the camera, and the lower side seen square to the axis, as netsmith render does', () => {
    expect(keepBelow([0, 0, 1], 'z')).toBe(true);
    expect(keepBelow([0, 0, -1], 'z')).toBe(false);
    expect(keepBelow([0, -1, 0], 'x')).toBe(true);
    expect(keepBelow([0, -1, 0], 'y')).toBe(false);
    expect(keepBelow([0.5, -0.5, 0.7], 'x')).toBe(true);
  });
  it('starts in the middle of the board and reads a typed position in mm, held to the range', () => {
    const b = { widthMm: 40, heightMm: 24, thicknessMm: 1.6 };
    expect([defaultCut('x', b), defaultCut('y', b), defaultCut('z', b)]).toEqual([20, 12, -0.8]);
    expect(parseCut('12.5', [0, 40])).toBe(12.5);
    expect(parseCut(' 7,25 mm', [0, 40])).toBe(7.25);
    expect(parseCut('80', [0, 40])).toBe(40);
    expect(parseCut('', [0, 40])).toBeNull();
    expect(parseCut('twelve', [0, 40])).toBeNull();
  });
  it('colours each layer of the cut as the render does: the mask, the core, the prepreg, and the insulator round the copper', () => {
    expect(two.map((_, i) => slabRole(two, i))).toEqual(['mask-top', 'mask-top', 'substrate', 'mask-bottom', 'mask-bottom']);
    const four: StackLayer[] = [
      two[0],
      two[1],
      { name: 'prepreg 1', kind: 'prepreg', thicknessMm: 0.2, zTopMm: -0.045, zBottomMm: -0.245 },
      { name: 'In1.Cu', kind: 'copper', thicknessMm: 0.015, zTopMm: -0.245, zBottomMm: -0.26 },
      { name: 'core', kind: 'core', thicknessMm: 1, zTopMm: -0.26, zBottomMm: -1.26 },
      { name: 'In2.Cu', kind: 'copper', thicknessMm: 0.015, zTopMm: -1.26, zBottomMm: -1.275 },
      { name: 'prepreg 2', kind: 'prepreg', thicknessMm: 0.2, zTopMm: -1.275, zBottomMm: -1.475 },
      { name: 'B.Cu', kind: 'copper', thicknessMm: 0.035, zTopMm: -1.475, zBottomMm: -1.51 },
      { name: 'B.Mask', kind: 'mask', thicknessMm: 0.01, zTopMm: -1.51, zBottomMm: -1.52 },
    ];
    expect(four.map((_, i) => slabRole(four, i))).toEqual(['mask-top', 'mask-top', 'prepreg', 'prepreg', 'substrate', 'prepreg', 'prepreg', 'mask-bottom', 'mask-bottom']);
  });
});

describe('the exploded stackup', () => {
  it('spreads the layers about the middle, top first, with the silkscreen outside the masks', () => {
    const slots = explodeSlots(two, { top: true, bottom: false }, 3);
    expect(slots.map((s) => s.key)).toEqual(['silk:top', 'F.Mask', 'F.Cu', 'core', 'B.Cu', 'B.Mask']);
    expect(slots.map((s) => s.offset)).toEqual([7.5, 4.5, 1.5, -1.5, -4.5, -7.5]);
    expect(slots[2].detail).toBe('copper · 0.035 mm');
    expect(slots[3].detail).toBe('core · 1.51 mm');
    expect(faceSlot(slots, 'top')?.key).toBe('silk:top');
    expect(faceSlot(slots, 'bottom')?.key).toBe('B.Mask');
  });
  it('a gap that reads on a small board and on a large one', () => {
    expect(explodeGap(10, 8)).toBe(3);
    expect(explodeGap(60, 40)).toBeCloseTo(6);
    expect(explodeGap(400, 300)).toBe(12);
  });
  it("a system's boards move apart with their stackups: the base stays, a board above moves up by both halves", () => {
    // The LED board 11 mm over the power board, the power board the base.
    const two = spreadMembers(
      [
        { key: 'led', z: 11, half: 7.5 },
        { key: 'power', z: 0, half: 9 },
      ],
      'power',
    );
    expect(Object.fromEntries(two)).toEqual({ power: 0, led: 16.5 });
    // Based on the top board, the one under it moves down; a board beside the base (drawn apart) moves with it.
    const three = spreadMembers(
      [
        { key: 'top', z: 11, half: 3 },
        { key: 'bottom', z: 0, half: 4 },
        { key: 'beside', z: 0.4, half: 6 },
      ],
      'top',
    );
    expect(Object.fromEntries(three)).toEqual({ bottom: -9, beside: -9, top: 0 });
  });
});

describe('copy the view as a netsmith render command', () => {
  it('names the nearest view of the command, and whether it is that view', () => {
    expect(nearestView([0, 0, 1])).toEqual({ view: 'top', exact: true });
    expect(nearestView([1, -1, 1])).toEqual({ view: 'iso', exact: true });
    expect(nearestView([1, -1, -1])).toEqual({ view: 'iso-bottom', exact: true });
    expect(nearestView([1, -1.3, 1.15])).toEqual({ view: 'iso', exact: false });
    expect(nearestView([0.1, -1, 0.05]).view).toBe('front');
  });
  it('quotes a word the shell would split', () => {
    expect(shellWord('src/main.nsm')).toBe('src/main.nsm');
    expect(shellWord('Demo/main')).toBe('Demo/main');
    expect(shellWord('my board.nsm')).toBe("'my board.nsm'");
    expect(shellWord("it's")).toBe("'it'\\''s'");
  });
  it('is the exact command for the view', () => {
    expect(renderCommand({ file: 'src/main.nsm', board: 'Lidded', view: 'iso' })).toBe('netsmith render src/main.nsm --board Lidded --view iso');
    expect(
      renderCommand({ file: 'src/main.nsm', board: 'Lidded', view: 'front', section: { axis: 'x', at: 12.34567 }, highlight: ['J1', 'U2', 'J1'], dark: true, perspective: true }),
    ).toBe('netsmith render src/main.nsm --board Lidded --view front --section x=12.346 --highlight J1,U2 --theme dark --perspective');
    // The case is named when it is not the command's default, see-through.
    const cased = { file: 'src/main.nsm', board: 'Cased', view: 'iso' as const };
    expect(renderCommand({ ...cased, case: 'see-through' })).toBe('netsmith render src/main.nsm --board Cased --view iso');
    expect(renderCommand({ ...cased, case: 'solid', highlight: ['case:lid'] })).toBe('netsmith render src/main.nsm --board Cased --view iso --highlight case:lid --case solid');
    expect(renderCommand({ ...cased, case: 'hidden', dark: true })).toBe('netsmith render src/main.nsm --board Cased --view iso --case hidden --theme dark');
    expect(renderCommand({ file: 'boards/a b.nsm', board: 'Sys/led', view: 'top', section: { axis: 'z', at: -0.8 } })).toBe(
      "netsmith render 'boards/a b.nsm' --board Sys/led --view top --section z=-0.8",
    );
  });
});
