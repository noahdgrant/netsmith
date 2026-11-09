// A comparison to test with: the shape the server answers (ADR 0059) for the small project of the recipe, written out.

import type { Compare, CompareBoard, ComparePcbPlace, CompareSystem, GitInfo, InstancesDiff, Location, PcbDiff } from '../api';

export const at = (line: number, file = 'src/main.nsm'): Location => ({ file, line, column: 5, endLine: line, endColumn: 20 });

const empty = (): InstancesDiff => ({ added: [], removed: [], changed: [] });

export function board(name: string, patch: Partial<CompareBoard> = {}): CompareBoard {
  return {
    name,
    status: 'unchanged',
    baseFile: 'src/main.nsm',
    headFile: 'src/main.nsm',
    stale: false,
    summary: '',
    parts: { added: [], removed: [], changed: [], renumbered: [], unchanged: 3 },
    nets: { added: [], removed: [], renamed: [], changed: [], unchanged: 4 },
    rails: { added: [], removed: [], changed: [], unchanged: 1 },
    modules: empty(),
    interfaces: empty(),
    bom: { added: [], removed: [], changed: [], unchanged: 2 },
    problems: { new: [], resolved: [], unchanged: 0 },
    ...patch,
  };
}

/** The led board: a resistor with another value. The power board: a capacitor added, a net renamed, a rail that lost headroom, a part removed. */
export function changed(): Compare {
  const led = board('TwoBoards/led', {
    status: 'changed',
    summary: '1 value changed',
    parts: {
      added: [],
      removed: [],
      changed: [
        { path: 'r1', designatorBase: 'R1', designatorHead: 'R1', kind: 'Resistor', fields: [{ field: 'value', old: '100', new: '150' }, { field: 'lcsc', old: 'C25076', new: 'C22775' }], at: at(21), baseAt: at(21) },
      ],
      renumbered: [],
      unchanged: 2,
    },
    bom: {
      added: [{ kind: 'Resistor', value: '150', footprint: '0603', lcsc: 'C22775', handSolder: false, quantity: 1, designators: ['R1'], unitUsd: 0.002 }],
      removed: [{ kind: 'Resistor', value: '100', footprint: '0603', lcsc: 'C25076', handSolder: false, quantity: 1, designators: ['R1'], unitUsd: 0.001 }],
      changed: [],
      unchanged: 1,
      totalUsdBase: 0.01,
      totalUsdHead: 0.011,
      deltaUsd: 0.001,
    },
  });
  const power = board('TwoBoards/power', {
    status: 'changed',
    summary: '1 part added',
    parts: {
      added: [{ path: 'stage.bulk', designator: 'C1', kind: 'Capacitor', value: '47u', package: '0805', lcsc: 'C16780', assembly: 'fab', at: at(24) }],
      removed: [{ path: 'stage.snubber', designator: 'C9', kind: 'Capacitor', value: '100n', assembly: 'fab', at: at(30) }],
      changed: [{ path: 'stage.regulator', basePath: 'ldo.regulator', designatorBase: 'U1', designatorHead: 'U1', kind: 'LinearRegulator', fields: [{ field: 'path', old: 'ldo.regulator', new: 'stage.regulator' }], at: at(12), baseAt: at(12) }],
      renumbered: [
        { path: 'stage.input_cap', designatorBase: 'C1', designatorHead: 'C2', kind: 'Capacitor', fields: [{ field: 'designator', old: 'C1', new: 'C2' }], at: at(25), baseAt: at(24) },
        { path: 'stage.output_cap', designatorBase: 'C2', designatorHead: 'C3', kind: 'Capacitor', fields: [{ field: 'designator', old: 'C2', new: 'C3' }], at: at(26), baseAt: at(25) },
      ],
      unchanged: 1,
    },
    nets: {
      added: [],
      removed: [],
      renamed: [{ name: 'VOUT', baseName: '3V3', renamed: true, joined: [], left: [], at: at(21) }],
      changed: [
        { name: 'VIN', baseName: 'VIN', renamed: false, joined: [{ part: 'stage.bulk', designator: 'C1', pin: '1', at: at(24) }], left: [], at: at(24) },
        { name: 'GND', baseName: 'GND', renamed: false, joined: [{ part: 'stage.bulk', designator: 'C1', pin: '2', at: at(24) }], left: [{ part: 'stage.snubber', designator: 'C9', pin: '2', at: at(30) }], at: at(24) },
      ],
      unchanged: 1,
    },
    rails: {
      added: [],
      removed: [],
      changed: [
        { net: 'VOUT', baseNet: '3V3', demand: { base: { lo: 0, hi: 0.05, text: '0A..50mA' }, head: { lo: 0, hi: 0.08, text: '0A..80mA' } }, headroomBase: 0.05, headroomHead: 0.02, at: at(21), baseAt: at(21) },
        { net: 'VIN', headroomBase: 0.02, headroomHead: 0.05, at: at(18), baseAt: at(18) },
        { net: 'AUX', budget: { base: 'within', head: 'over' }, at: at(19), baseAt: at(19) },
      ],
      unchanged: 0,
    },
    modules: { added: [{ path: 'stage', typeName: 'LdoStage', at: at(10) }], removed: [], changed: [{ path: 'filter', baseType: 'Pi', headType: 'Lc', at: at(14) }] },
    problems: {
      new: [{ severity: 'warning', message: 'stage.bulk: no LCSC number\nmore', rendered: '', at: at(24), stage: 'design', board: 'TwoBoards/power', path: 'stage.bulk' }],
      resolved: [{ severity: 'error', message: 'C9 is not placed', rendered: '', at: at(30), stage: 'design', board: 'TwoBoards/power' }],
      unchanged: 0,
    },
  });
  return {
    ref: 'main',
    base: { hash: '86d3736cc7b9f7660d36b29acc324b12ee90fd7b', short: '86d3736', subject: 'First', author: 'Ada', date: '2026-11-06T02:46:58-05:00' },
    head: { commit: undefined, dirty: true, errors: 0 },
    baseProblems: [],
    cached: false,
    elapsedMs: 868,
    summary: '1 part added, 1 removed, 2 values changed',
    changed: true,
    source: { filesChanged: 1, added: 5, removed: 3, files: [{ path: 'src/main.nsm', status: 'modified', added: 5, removed: 3 }] },
    boards: [led, power],
    systems: [],
    problems: { new: [], resolved: [], unchanged: 0 },
  };
}

const place = (x: number, y: number, placement = 'hand'): ComparePcbPlace => ({ x, y, rotation: 0, side: 'top', locked: false, placement });

/** What moved on the PCB of the led board: the resistor 2.5 mm to the right and turned, the outline stated and bigger, two more layers,
 * a keepout that grew and one that is new, a cutout that is gone. */
export function pcbChanged(): PcbDiff {
  const box = (x: number, w: number) => ({ x, y: 1, width: w, height: 2 });
  return {
    layoutFile: 'src/main.pcb.toml',
    outline: { base: { width: 30, height: 20 }, head: { width: 32, height: 20 } },
    layers: { base: ['top', 'bottom'], head: ['top', 'inner1', 'inner2', 'bottom'] },
    moved: [
      {
        path: 'r1',
        designatorBase: 'R1',
        designatorHead: 'R1',
        base: place(5, 2),
        head: { ...place(7.5, 2), rotation: 90 },
        fields: [
          { field: 'position', old: '5, 2 mm', new: '7.5, 2 mm' },
          { field: 'rotation', old: '0°', new: '90°' },
        ],
        distanceMm: 2.5,
      },
    ],
    keepouts: [
      { name: 'antenna', status: 'changed', base: box(1, 4), head: box(1, 6) },
      { name: 'rf', status: 'added', head: box(8, 2) },
    ],
    cutouts: [{ name: 'slot', status: 'removed', base: box(3, 2) }],
    unchanged: 2,
  };
}

/** The system of the two boards: its output port's current lowered, a join and a mate gone, a ground group new, a rail only it explains. */
export function systemChanged(): CompareSystem {
  return {
    name: 'TwoBoards',
    status: 'changed',
    baseFile: 'src/main.nsm',
    headFile: 'src/main.nsm',
    summary: '1 system port changed',
    boards: { added: [{ path: 'aux', typeName: 'AuxBoard', at: at(110) }], removed: [], changed: [{ path: 'led', baseType: 'LedBoard', headType: 'LedBoard2', at: at(104) }] },
    ports: { added: [], removed: [{ board: 'led', name: 'spare', voltage: '3.3V' }], changed: [{ board: 'power', name: 'output', fields: [{ field: 'current', old: '0mA..50mA', new: '0mA..30mA' }] }] },
    joins: { added: ['power.aux ~ aux.input'], removed: [] },
    mates: {
      added: [],
      removed: [{ a: 'power.j2', b: 'led.j2', ports: ['power.x ~ led.x'], at: at(106) }],
      changed: [{ a: 'power.power_out.mating', b: 'led.power_in.mating', fields: [{ field: 'contact 2', old: 'ground to ground', new: 'signal to ground' }], at: at(108) }],
    },
    groundGroups: { added: ['power.GND + led.GND + aux.GND'], removed: [] },
    rails: { added: [], removed: [], changed: [{ net: 'led:VIN', headroomBase: 0.03, headroomHead: 0.01, budget: { base: 'within', head: 'over' }, at: at(112), baseAt: at(112) }], unchanged: 0 },
  };
}

/** `changed()` with a PCB on the led board and the system. */
export function changedWithSystemAndPcb(): Compare {
  const c = changed();
  c.boards[0] = { ...c.boards[0], pcb: pcbChanged() };
  c.systems = [systemChanged()];
  return c;
}

export function info(patch: Partial<GitInfo> = {}): GitInfo {
  const c = (n: number, subject: string, date = '2026-11-05T10:00:00+01:00') => ({ hash: `${n}`.repeat(40).slice(0, 40), short: `${n}`.repeat(7), subject, author: 'Ada', date });
  const ref = (name: string, kind: GitInfo['refs'][number]['kind'], n: number, current = false) => ({ name, kind, hash: `${n}`.repeat(40).slice(0, 40), short: `${n}`.repeat(7), subject: `${name} subject`, date: '2026-11-05T10:00:00+01:00', current });
  return {
    available: true,
    root: '/work/board',
    projectDir: 'hardware',
    branch: 'feature',
    head: c(4, 'Tune the LED'),
    dirty: true,
    projectDirty: true,
    refs: [ref('HEAD', 'head', 4), ref('origin/main', 'upstream', 3), ref('main', 'default', 3), ref('feature', 'branch', 4, true), ref('broken', 'branch', 5), ref('v1.0', 'tag', 1)],
    commits: [c(4, 'Tune the LED'), c(2, 'Second'), c(1, 'First')],
    ...patch,
  };
}
