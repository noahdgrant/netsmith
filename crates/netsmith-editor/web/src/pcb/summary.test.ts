import { describe, expect, it } from 'vitest';

import type { CopperLayer, CopperZone, PcbCopper, PcbScene, SummaryGuideline, SummaryPads } from '../api';
import { copperLayerLine, copperZoneLine, fillWord, guidelineLine, mm, padName, padProblem, padsLine, pourFields, sheetBox, summaryQuery, teardropsLine } from './summary';

describe('a number of the summary', () => {
  it('is to the hundredth with no trailing zeros', () => {
    expect(mm(3.5)).toBe('3.5');
    expect(mm(12)).toBe('12');
    expect(mm(0.123)).toBe('0.12');
    expect(fillWord(0.344)).toBe('34%');
  });
});

describe('a box of the summary', () => {
  it('moves by the outline origin to the sheet', () => {
    const scene = { originX: 100, originY: 50 } as PcbScene;
    expect(sheetBox(scene, { min: [1, 2], max: [4, 8] })).toEqual({ x: 101, y: 52, width: 3, height: 6 });
  });
});

describe('a pad named by hand', () => {
  it('is a part, a dot and the pad number', () => {
    expect(padName('R1.2')).toBe('R1.2');
    expect(padName(' stage.c1.1 ')).toBe('stage.c1.1');
    expect(padName('R1')).toBeNull();
    expect(padName('R1.')).toBeNull();
    expect(padName('.2')).toBeNull();
    expect(padName('R1.1,R1.2')).toBeNull();
  });
});

describe('the summary query', () => {
  it('repeats near and between and gives within only with near', () => {
    expect(summaryQuery('Led', [], 2, [])).toBe('board=Led');
    expect(summaryQuery('Led', ['R1', 'a.b'], 3.5, [['R1.1', 'U1.3']])).toBe('board=Led&near=R1&near=a.b&within=3.5&between=R1.1%2CU1.3');
  });
});

describe('the distance between two pads in words', () => {
  const base: SummaryPads = { from: 'R1.1', to: 'R1.2', distance: 1.5, dx: 1.5, dy: 0, gap: 0.4 };
  it('says the net they share', () => {
    expect(padsLine({ ...base, fromNet: 'VCC', toNet: 'VCC' })).toBe('1.5 mm between centres, 0.4 mm between the copper, both on VCC');
  });
  it('says each net when they differ, and nothing for pads on none', () => {
    expect(padsLine({ ...base, fromNet: 'A', toNet: 'B' })).toBe('1.5 mm between centres, 0.4 mm between the copper, R1.1 on A, R1.2 on B');
    expect(padsLine(base)).toBe('1.5 mm between centres, 0.4 mm between the copper');
  });
});

describe('a pad the board does not have', () => {
  const scene = { board: 'Led', parts: [{ refdes: 'R1', path: 'stage.r1', pads: [{ number: '1' }, { number: '2' }] }] } as unknown as PcbScene;
  it('is told in the server\'s words, with the pads the part has', () => {
    expect(padProblem(scene, 'R1.2')).toBeNull();
    expect(padProblem(scene, 'stage.r1.1')).toBeNull();
    expect(padProblem(scene, 'R1.9')).toBe('the part `R1` has no pad `9`; its pads are: 1, 2');
    expect(padProblem(scene, 'R7.1')).toBe('the board `Led` has no part `R7`');
  });
});

describe('a datasheet guideline of the summary', () => {
  const g = (more: Partial<SummaryGuideline>): SummaryGuideline => ({ part: 'reg.ic', name: 'input_cap', statement: 'near', source: 'SNVSA24, p.17', ...more });
  it('says what it measures, the number and whether it is met', () => {
    expect(guidelineLine(g({ other: 'C1', measured: 1.234, atMost: 2, met: true }))).toBe('(near, C1) 1.23 mm, at most 2 mm, met');
    expect(guidelineLine(g({ statement: 'away', other: 'L1', measured: 3, atLeast: 5, met: false }))).toBe('(away, L1) 3 mm, at least 5 mm, not met');
  });
  it('counts vias with no unit, and says when nothing is measured', () => {
    expect(guidelineLine(g({ statement: 'vias', measured: 2, atLeast: 4, met: false }))).toBe('(vias) 2, at least 4, not met');
    expect(guidelineLine(g({ statement: 'edge' }))).toBe('(edge) not measured');
  });
});

describe('the copper areas', () => {
  const pour = { stitchPitch: 7, thermalGap: null, spokeWidth: null, clearance: 0.3, minWidth: null, pads: 'solid' };
  const layer = (over: Partial<CopperLayer>): CopperLayer => ({ layer: 'top', kind: 'signal', net: null, pour: null, zones: 0, replacedByZones: false, ...over });
  it('say a pour field the design does not state is the default', () => {
    expect(pourFields(pour)).toBe('stitch pitch 7 mm, thermal gap default, spoke width default, clearance 0.3 mm, min width default, pads solid');
  });
  it('tell a plane, a pour and a signal layer apart, and what hand zones do to them', () => {
    expect(copperLayerLine(layer({ kind: 'signal' }))).toBe('signal');
    expect(copperLayerLine(layer({ kind: 'plane', net: 'GND' }))).toBe('plane on GND');
    expect(copperLayerLine(layer({ kind: 'pour', net: 'GND', pour }))).toMatch(/^pour on GND: stitch pitch 7 mm/);
    expect(copperLayerLine(layer({ kind: 'signal', zones: 2 }))).toBe('signal (2 hand zones)');
    expect(copperLayerLine(layer({ kind: 'plane', net: 'GND', zones: 1, replacedByZones: true }))).toBe('plane on GND (not drawn: 1 zone of the routes file replaces it)');
  });
  it('write a zone of the routes file on one line', () => {
    const zone: CopperZone = { net: 'GND', layers: ['top'], priority: 0, clearance: 0.2, minWidth: 0.25, pads: 'thermal', thermalGap: 0.5, spokeWidth: 0.5, islands: 'remove', area: 24, locked: true };
    expect(copperZoneLine(zone)).toBe('GND on top: 24 mm², priority 0, clearance 0.2 mm, min width 0.25 mm, pads thermal, locked');
  });
  it('count the teardrop fills only for a routed board', () => {
    const copper = (teardrops: PcbCopper['teardrops']) => ({ board: 'B', layers: [], zones: [], teardrops }) as PcbCopper;
    expect(teardropsLine(copper({ counted: false, total: 0, layers: [] }))).toMatch(/^not counted/);
    expect(teardropsLine(copper({ counted: true, total: 3, layers: [{ layer: 'top', count: 2 }, { layer: 'bottom', count: 1 }] }))).toBe('3 (2 on top, 1 on bottom)');
    expect(teardropsLine(copper({ counted: true, total: 0, layers: [] }))).toBe('0');
  });
});
