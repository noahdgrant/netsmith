import { describe, expect, it } from 'vitest';

import type { CopperLayer, CopperZone, FillArea, FillCounts, FillPad, FillRegulatorGround, NetsGroup, NetsPair, NetsRow, NetsUncounted, PcbCopper, PcbScene, SummaryGuideline, SummaryPads } from '../api';
import { copperLayerLine, copperZoneLine, fillAreaLine, fillCountsLine, fillPadLine, fillRegulatorGroundLine, fillStarvedLine, fillWord, guidelineLine, mm, netGroupLine, netPairLine, netRowLine, netUncountedLine, uncountedBox, uncountedRunLine, padName, padProblem, padsLine, pairsOnBoard, pourFields, sheetBox, summaryQuery, teardropsLine } from './summary';

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
  it('leaves a pair of pads out, saying why, while the board has no such pad', () => {
    expect(pairsOnBoard(scene, [['R7.1', 'R1.1'], ['R1.1', 'R1.2'], ['R1.1', 'stage.r1.9']])).toEqual({
      asked: [['R1.1', 'R1.2']],
      leftOut: [
        'R7.1 to R1.1 is not measured: the board `Led` has no part `R7`',
        'R1.1 to stage.r1.9 is not measured: the part `R1` has no pad `9`; its pads are: 1, 2',
      ],
    });
    expect(pairsOnBoard(scene, [])).toEqual({ asked: [], leftOut: [] });
  });
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
  const layer = (over: Partial<CopperLayer>): CopperLayer => ({ layer: 'top', kind: 'signal', net: null, pour: null, zones: 0, replacedByZones: false, patchedByZones: false, ...over });
  it('say a pour field the design does not state is the default', () => {
    expect(pourFields(pour)).toBe('stitch pitch 7 mm, thermal gap default, spoke width default, clearance 0.3 mm, min width default, pads solid');
  });
  it('tell a plane, a pour and a signal layer apart, and what hand zones do to them', () => {
    expect(copperLayerLine(layer({ kind: 'signal' }))).toBe('signal');
    expect(copperLayerLine(layer({ kind: 'plane', net: 'GND' }))).toBe('plane on GND');
    expect(copperLayerLine(layer({ kind: 'pour', net: 'GND', pour }))).toMatch(/^pour on GND: stitch pitch 7 mm/);
    expect(copperLayerLine(layer({ kind: 'signal', zones: 2 }))).toBe('signal (2 hand zones)');
    expect(copperLayerLine(layer({ kind: 'pour', net: 'GND', zones: 1 }))).toBe('pour on GND (cut by 1 hand zone of other nets, drawn round them)');
    expect(copperLayerLine(layer({ kind: 'pour', net: 'GND', zones: 1, patchedByZones: true }))).toBe('pour on GND (cut or patched by 1 hand zone, drawn round them)');
    expect(copperLayerLine(layer({ kind: 'plane', net: 'GND', zones: 1, replacedByZones: true }))).toBe('plane on GND (not drawn: 1 zone of the routes file replaces it)');
  });
  it('write a zone of the routes file on one line', () => {
    const zone: CopperZone = { net: 'GND', layers: ['top'], priority: 0, clearance: 0.2, minWidth: 0.25, pads: 'thermal', thermalGap: 0.5, spokeWidth: 0.5, islands: 'remove', area: 24, locked: true, patch: false };
    expect(copperZoneLine(zone)).toBe('GND on top: 24 mm², priority 0, clearance 0.2 mm, min width 0.25 mm, pads thermal, locked');
  });
  it('count the teardrop fills only for a routed board', () => {
    const copper = (teardrops: PcbCopper['teardrops']) => ({ board: 'B', layers: [], zones: [], teardrops }) as PcbCopper;
    expect(teardropsLine(copper({ counted: false, total: 0, layers: [] }))).toMatch(/^not counted/);
    expect(teardropsLine(copper({ counted: true, total: 3, layers: [{ layer: 'top', count: 2 }, { layer: 'bottom', count: 1 }] }))).toBe('3 (2 on top, 1 on bottom)');
    expect(teardropsLine(copper({ counted: true, total: 0, layers: [] }))).toBe('0');
  });
  it('count the pads a fill joins by kind, and say how each is joined', () => {
    const counts: FillCounts = { noSpokes: 0, oneSpoke: 1, twoOrMoreSpokes: 2, solid: 1, none: 0, starved: 1, unjoinable: 0 };
    expect(fillCountsLine(counts)).toBe('4 pads: 2 by 2 or more spokes, 1 by 1 spoke, 0 by no spoke, 1 solid, 0 none; 1 starved, 0 unjoinable');
    const pad = (p: Partial<FillPad>): FillPad => ({ part: 'U1', pad: '2', at: { x: 8, y: 15 }, join: 'thermal', spokes: 4, joinedSpokes: 1, tracks: 0, joined: true, starved: true, unjoinable: null, joinedElsewhere: false, statedBy: null, spokeWidth: null, ...p });
    expect(fillPadLine(pad({}))).toBe('U1.2: thermal, 1 of 4 spokes (starved)');
    expect(fillPadLine(pad({ joinedSpokes: 1, tracks: 1, starved: false }))).toBe('U1.2: thermal, 1 of 4 spokes and 1 track');
    expect(fillPadLine(pad({ join: 'solid', spokes: null, joinedSpokes: null, starved: false }))).toBe('U1.2: solid');
    expect(fillPadLine(pad({ join: 'solid', spokes: null, joinedSpokes: null, joined: false, starved: false }))).toBe('U1.2: solid, the fill does not reach it');
    expect(fillPadLine(pad({ joinedSpokes: 0, starved: false, unjoinable: 'the edge' }))).toBe('U1.2: thermal, 0 of 4 spokes (unjoinable: the edge)');
    expect(fillPadLine(pad({ join: 'none', spokes: null, joinedSpokes: null, joined: false, starved: false }))).toBe('U1.2: none');
    expect(fillPadLine(pad({ join: 'solid', spokes: null, joinedSpokes: null, starved: false, statedBy: 'radio.gnd' }))).toBe('U1.2: solid (as its pin radio.gnd states)');
    expect(fillPadLine(pad({ statedBy: 'radio.shield', spokeWidth: 0.3 }))).toBe('U1.2: thermal, 1 of 4 spokes (as its pin radio.shield states, 0.3 mm spokes) (starved)');
    expect(fillPadLine(pad({ joinedSpokes: 0, joined: false, starved: false, joinedElsewhere: true }))).toBe('U1.2: thermal, 0 of 4 spokes (joined by another fill of its net)');
    const area: FillArea = { layer: 'top', layerName: 'F.Cu', net: 'GND', source: 'pour', held: false, area: 120.456, pads: [], counts, cleared: [], islands: [], removedIslands: [{ at: { x: 1, y: 2 }, area: 0.5, outline: [] }] };
    expect(fillAreaLine(area)).toBe(`pour on GND, 120.46 mm²: ${fillCountsLine(counts)}; clears 0 pads of other nets; islands: 0 kept, 1 removed`);
    expect(fillStarvedLine({ layer: 'top', net: 'GND', part: 'U1', pad: '2', at: { x: 8, y: 15 }, spokes: 4, joinedSpokes: 1, tracks: 0 })).toBe('U1.2 on top (GND) at (8, 15): 1 of 4 spokes');
  });
  it('warn of a regulator ground a fill does not join solid, and name the pin to state it on', () => {
    const ground: FillRegulatorGround = { path: 'reg.ic', layer: 'top', net: 'GND', part: 'U1', pad: '2', at: { x: 8, y: 15 }, join: 'thermal', spokes: 4, joinedSpokes: 4, starved: false, statedBy: null };
    expect(fillRegulatorGroundLine(ground)).toBe(
      'U1.2 on top (GND) at (8, 15), the ground of the switching regulator reg.ic: a thermal relief, 4 of 4 spokes; a regulator\u2019s ground carries its switched current and its heat: join it solid with .pour_connection &= PadConnection::Solid on its pin reg.ic.vin.neg',
    );
    expect(fillRegulatorGroundLine({ ...ground, joinedSpokes: 1, starved: true })).toContain('a thermal relief, 1 of 4 spokes, starved;');
    expect(fillRegulatorGroundLine({ ...ground, join: 'none', spokes: null, joinedSpokes: null })).toContain('not joined (the fill\u2019s pad connection is none);');
    expect(fillRegulatorGroundLine({ ...ground, join: 'solid', spokes: null, joinedSpokes: null })).toContain('solid, but the fill does not reach it;');
    expect(fillRegulatorGroundLine({ ...ground, statedBy: 'reg.ic.gnd' }).endsWith('on its pin reg.ic.gnd')).toBe(true);
  });
});

describe('the table of the nets', () => {
  it('says a net and a pair in words', () => {
    const row: NetsRow = { net: 'CANH', length: 63.094, copper: 63.094, straight: 60.2, ratio: 1.0481, vias: 2, layerChanges: 2, layers: ['F.Cu', 'B.Cu'], notCounted: [] };
    expect(netRowLine(row)).toBe('CANH: 63.09 mm routed, straight 60.2 mm (1.05x), 2 vias, 2 layer changes, on F.Cu B.Cu');
    expect(netRowLine({ ...row, ratio: 0, vias: 1, layerChanges: 1, layers: [] })).toBe('CANH: 63.09 mm routed, straight 60.2 mm, 1 via, 1 layer change');
    const pair: NetsPair = { p: 'CANH', n: 'CANL', lengthP: 63.09, lengthN: 63.11, skew: 0.02, limit: 0.1, over: false, coupled: 60.5, share: 0.959, gaps: [0.1, 0.12], viasP: 2, viasN: 2, viaPairs: 2 };
    expect(netPairLine(pair)).toBe('CANH/CANL: 63.09 and 63.11 mm, skew 0.02 mm (within 0.1 mm); coupled 60.5 mm (96%), gap 0.1 to 0.12 mm; vias 2 and 2, 2 side by side');
    expect(netPairLine({ ...pair, skew: 2.5, over: true, coupled: 0, share: 0, gaps: null })).toBe('CANH/CANL: 63.09 and 63.11 mm, skew 2.5 mm (over 0.1 mm); coupled 0 mm (0%); vias 2 and 2, 2 side by side');
  });

  it('says the copper a net leaves out of its length, and where it is (issue 1495)', () => {
    const run: NetsUncounted = { kind: 'second_way', layer: 'F.Cu', length: 10, at: [12, 10], pieces: [{ from: [12, 10], to: [17, 10], width: 0.2 }, { from: [17, 10], to: [17, 15], width: 0.2 }] };
    const row: NetsRow = { net: 'P', length: 20, copper: 30, straight: 20, ratio: 1, vias: 0, layerChanges: 0, layers: ['F.Cu'], notCounted: [run] };
    expect(netUncountedLine(row)).toBe('P: 10 mm of its 30 mm of copper not counted in its length');
    expect(netUncountedLine({ ...row, notCounted: [] })).toBe('');
    expect(uncountedRunLine(run)).toBe('a second way of 10 mm on F.Cu at (12, 10)');
    expect(uncountedRunLine({ ...run, kind: 'stub', length: 1.5 })).toBe('a dead stub of 1.5 mm on F.Cu at (12, 10)');
    expect(uncountedBox(run)).toEqual({ x: 11, y: 9, width: 7, height: 7 });
  });

  it('says a group of nets matched in length in words', () => {
    const group: NetsGroup = {
      path: 'lanes',
      nets: [
        { net: 'D0', length: 40.1, shortBy: 0 },
        { net: 'D1', length: 39.8, shortBy: 0.3 },
      ],
      spread: 0.3,
      tolerance: 0.5,
      over: false,
    };
    expect(netGroupLine(group)).toBe('lanes: D0 40.1, D1 39.8 mm, spread 0.3 mm (within 0.5 mm)');
    expect(netGroupLine({ ...group, tolerance: 0.2, over: true })).toBe('lanes: D0 40.1, D1 39.8 mm, spread 0.3 mm (over 0.2 mm)');
  });
});
