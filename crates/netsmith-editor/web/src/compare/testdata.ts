// A comparison to test with: the shape the server answers (ADR 0051) for the small project of the recipe, written out.

import type { Compare, CompareBoard, GitInfo, InstancesDiff, Location } from '../api';

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
        { path: 'r1', designatorBase: 'R1', designatorHead: 'R1', fields: [{ field: 'value', old: '100', new: '150' }, { field: 'lcsc', old: 'C25076', new: 'C22775' }], at: at(21), baseAt: at(21) },
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
      changed: [{ path: 'stage.regulator', basePath: 'ldo.regulator', designatorBase: 'U1', designatorHead: 'U1', fields: [{ field: 'path', old: 'ldo.regulator', new: 'stage.regulator' }], at: at(12), baseAt: at(12) }],
      renumbered: [
        { path: 'stage.input_cap', designatorBase: 'C1', designatorHead: 'C2', fields: [{ field: 'designator', old: 'C1', new: 'C2' }], at: at(25), baseAt: at(24) },
        { path: 'stage.output_cap', designatorBase: 'C2', designatorHead: 'C3', fields: [{ field: 'designator', old: 'C2', new: 'C3' }], at: at(26), baseAt: at(25) },
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
      new: [{ severity: 'warning', message: 'stage.bulk: no LCSC number\nmore', rendered: '', at: at(24), stage: 'lower', board: 'TwoBoards/power', path: 'stage.bulk' }],
      resolved: [{ severity: 'error', message: 'C9 is not placed', rendered: '', at: at(30), stage: 'lower', board: 'TwoBoards/power' }],
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
    problems: { new: [], resolved: [], unchanged: 0 },
  };
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
