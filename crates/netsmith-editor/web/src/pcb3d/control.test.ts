import { describe, expect, it } from 'vitest';

import { type Ids, ViewRefusal, litIds, memberSelection, parseSection, planChange, resolveIds, sectionText } from './control';
import { renderCommand } from './tools';

const have: Ids = {
  parts: [
    { path: 'power_in', refdes: 'J1' },
    { path: 'status.led', refdes: 'D1' },
    { path: 'r1', refdes: 'R1' },
  ],
  nets: ['VBUS', 'GND', 'GND'],
  limits: ['lid', 'lid'],
  cases: ['base', 'base', 'top'],
};

const refusal = (f: () => unknown): ViewRefusal => {
  try {
    f();
  } catch (e) {
    if (e instanceof ViewRefusal) return e;
    throw e;
  }
  throw new Error('expected a refusal');
};

describe('the section, as render --section reads and writes it', () => {
  it('reads an axis and a position, and none', () => {
    expect(parseSection('x=13')).toEqual({ axis: 'x', at: 13 });
    expect(parseSection(' z = -0.8 ')).toEqual({ axis: 'z', at: -0.8 });
    expect(parseSection('none')).toBeNull();
  });
  it('refuses what is not one, saying what it wants', () => {
    for (const bad of ['13', 'w=3', 'x=', 'x=abc', 'x=1,y=2']) expect(refusal(() => parseSection(bad)).message).toContain('such as x=12.5');
  });
  it('writes a cut the way it reads one', () => {
    expect(sectionText({ axis: 'y', at: 3.8712345 })).toBe('y=3.871');
    expect(sectionText(null)).toBeNull();
    expect(parseSection(sectionText({ axis: 'x', at: 7 })!)).toEqual({ axis: 'x', at: 7 });
  });
});

describe('the ids of --highlight', () => {
  it('finds a part by designator or path, a net and a limit, each once, comma-separated or not', () => {
    expect(resolveIds(['J1', 'status.led,net:GND', 'limit:lid', 'power_in'], have)).toEqual({ parts: ['power_in', 'status.led'], nets: ['GND'], limits: ['lid'], cases: [] });
  });
  it('finds a case model by name, as render --highlight case:<name> does', () => {
    expect(resolveIds(['case:base,case:top', 'case:base'], have)).toEqual({ parts: [], nets: [], limits: [], cases: ['base', 'top'] });
  });
  it('names the ids the board does not have and lists what it has, as render does', () => {
    const e = refusal(() => resolveIds(['J9', 'net:NOPE', 'R1', 'case:lid'], have));
    expect(e.status).toBe(400);
    expect(e.message).toBe(
      'highlight names ids that the board does not have: `J9`, `net:NOPE`, `case:lid`\n' +
        'the parts are: J1 (power_in), D1 (status.led), R1 (r1)\n' +
        'the nets: net:GND, net:VBUS\n' +
        'the limits: limit:lid\n' +
        'the case models: case:base, case:top',
    );
    const none = refusal(() => resolveIds(['x'], { parts: [], nets: [], limits: [], cases: [] })).message;
    expect(none).toContain('an id that the board does not have: `x`\nthe parts are: (none)');
    expect(none).toContain('the case models: (none)');
  });
});

describe("the ids of a system's scene (ADR 0110)", () => {
  it('refuses a case model: a case is one board\'s, and the system draws none (ADR 0109)', () => {
    const sys: Ids = { parts: [], nets: [], limits: [], cases: [], system: 'TwoBoards' };
    const why = refusal(() => resolveIds(['case:lid'], sys)).message;
    expect(why).toContain('names `case:lid`, a case model, and the system `TwoBoards` draws none');
    expect(why).toContain('--board <System>/<member>');
  });

  const system: Ids = {
    parts: [
      { path: 'power/power_out', refdes: 'power/J1' },
      { path: 'led/power_in', refdes: 'led/J1' },
    ],
    nets: ['led/GND', 'power/GND'],
    limits: [],
    cases: [],
    system: 'TwoBoards',
  };
  it("names a member's part and net, the net also as render writes it", () => {
    expect(resolveIds(['led/J1', 'power/power_out'], system)).toEqual({ parts: ['led/power_in', 'power/power_out'], nets: [], limits: [], cases: [] });
    expect(resolveIds(['net:led/GND'], system).nets).toEqual(['led/GND']);
    expect(resolveIds(['led/net:GND'], system).nets).toEqual(['led/GND']);
  });
  it('refuses a designator without its member, listing the members\' ids', () => {
    const e = refusal(() => resolveIds(['J1', 'net:GND'], system));
    expect(e.message).toBe(
      'highlight names ids that the system `TwoBoards` does not have: `J1`, `net:GND`\n' +
        'the parts are: power/J1 (power/power_out), led/J1 (led/power_in)\n' +
        'the nets: net:led/GND, net:power/GND\n' +
        'the limits: (none)\n' +
        "(in the system, a part and a net are a member's: such as power/J1 and net:led/GND)",
    );
  });
  it("picks on one member's board, by its own names", () => {
    expect(memberSelection({ parts: ['led/power_in'], net: null }, 'select')).toEqual({ member: 'led', parts: ['power_in'], net: null });
    expect(memberSelection({ parts: [], net: 'power/GND' }, 'select')).toEqual({ member: 'power', parts: [], net: 'GND' });
    expect(memberSelection({ parts: [], net: null }, 'select')).toEqual({ member: null, parts: [], net: null });
    expect(refusal(() => memberSelection({ parts: ['led/power_in', 'power/power_out'], net: null }, 'highlight')).message).toContain('highlight names ids of 2 members (led, power)');
  });
});

describe('a change, checked before anything is done', () => {
  it('maps the names of render to what the view does', () => {
    const p = planChange({ view: 'iso-bottom', section: 'x=13', highlight: ['J1', 'limit:lid'], xray: true, perspective: true, colourBy: 'margin', heightPreview: 2.345, exploded: true }, have, true);
    expect(p.view).toBe('iso-bottom');
    expect(p.towardEye![2]).toBeLessThan(0);
    expect(p.section).toEqual({ axis: 'x', at: 13 });
    expect(p.selection).toEqual({ parts: ['power_in'], net: null });
    expect(p.limits).toEqual(['lid']);
    expect(p).toMatchObject({ xray: true, ortho: false, colourBy: 'margin', preview: { on: true, mm: 2.35 }, exploded: true });
  });
  it('leaves alone what a change does not name', () => {
    expect(planChange({}, have, true)).toEqual({});
    expect(planChange({ section: 'none', highlight: [] }, have, true)).toEqual({ section: null, selection: { parts: [], net: null }, limits: [], cases: [] });
    expect(planChange({ heightPreview: false }, have, true).preview).toEqual({ on: false });
  });
  it('takes how the case is drawn, on a board that has one', () => {
    for (const mode of ['hidden', 'see-through', 'solid'] as const) expect(planChange({ case: mode }, have, true).case).toBe(mode);
    expect(planChange({}, have, true).case).toBeUndefined();
    const none = refusal(() => planChange({ case: 'solid' }, { ...have, cases: [] }, true));
    expect(none.message).toBe('case: the board has no case model to draw');
    expect(none.status).toBe(409);
    const sys = refusal(() => planChange({ case: 'hidden' }, { ...have, cases: [], system: 'TwoBoards' }, true)).message;
    expect(sys).toContain('the system `TwoBoards` draws no case model');
  });
  it('lights one net through the selection, and not with parts', () => {
    expect(planChange({ select: ['net:GND'] }, have, true).selection).toEqual({ parts: [], net: 'GND' });
    expect(refusal(() => planChange({ highlight: ['net:GND', 'net:VBUS'] }, have, true)).message).toContain('one net at a time');
    expect(refusal(() => planChange({ select: ['J1', 'net:GND'] }, have, true)).message).toContain('parts or one net');
    expect(refusal(() => planChange({ select: ['limit:lid'] }, have, true)).message).toContain('use highlight');
    expect(refusal(() => planChange({ select: ['case:base'] }, have, true)).message).toContain('a case model is not selected');
    expect(planChange({ highlight: ['J1', 'case:top'] }, have, true)).toMatchObject({ selection: { parts: ['power_in'] }, limits: [], cases: ['top'] });
  });
  it('refuses a camera that is not a place, a preview out of range and a stackup the board has not', () => {
    expect(refusal(() => planChange({ camera: { position: [1, 2, 3], target: [1, 2, 3] } }, have, true)).message).toContain('the same point');
    expect(refusal(() => planChange({ camera: { position: [1, 2] as never, target: [0, 0, 0] } }, have, true)).message).toContain('three numbers');
    expect(planChange({ camera: { position: [10, -20, 30], target: [5, 5, 0] } }, have, true).camera).toEqual({ position: [10, -20, 30], target: [5, 5, 0] });
    expect(refusal(() => planChange({ heightPreview: 99 }, have, true)).message).toContain('0.1 to 50 mm');
    const e = refusal(() => planChange({ exploded: true }, have, false));
    expect(e.status).toBe(409);
    expect(planChange({ exploded: false }, have, false).exploded).toBe(false);
  });
});

describe('what is lit, as the state and the copied command name it', () => {
  it('names the parts by designator, the net and the limits', () => {
    expect(litIds(['power_in', 'r1'], null, ['lid', 'lid'], have.parts)).toEqual({ selection: ['J1', 'R1'], highlight: ['J1', 'R1', 'limit:lid'] });
    expect(litIds([], 'GND', [], have.parts)).toEqual({ selection: ['net:GND'], highlight: ['net:GND'] });
  });
  it('gives the ids a change takes back: the copied command and a change agree', () => {
    const lit = litIds(['status.led'], null, ['lid'], have.parts, ['base', 'base']);
    expect(lit.highlight).toEqual(['D1', 'limit:lid', 'case:base']);
    const cmd = renderCommand({ file: 'src/main.nsm', board: 'B', view: 'iso', highlight: lit.highlight });
    expect(cmd).toContain("--highlight D1,limit:lid,case:base");
    expect(planChange({ highlight: lit.highlight }, have, true)).toMatchObject({ selection: { parts: ['status.led'] }, limits: ['lid'], cases: ['base'] });
  });
});
