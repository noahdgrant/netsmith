import { describe, expect, it } from 'vitest';

import type { Location, PcbFinding, Problem } from '../api';
import {
  DEFAULT_FILTER,
  type Row,
  classify,
  countRows,
  filterRows,
  groupRows,
  inScope,
  instancePath,
  nextRow,
  rowOfFinding,
  rowOfPowerIssue,
  rowOfProblem,
  sortRows,
  splitBoard,
  steps,
  summaryText,
} from './problems';

const at = (file: string, line: number, column = 1): Location => ({ file, line, column, endLine: line, endColumn: column + 1 });
const problem = (severity: Problem['severity'], message: string, where: Location | null = null): Problem => ({
  severity,
  message,
  rendered: `${severity}: ${message}\n`,
  at: where,
});
const rows = (ps: Problem[]) => ps.map(rowOfProblem);

describe('classify', () => {
  it('knows the stages by what they say', () => {
    expect(classify(problem('error', 'this `{` is never closed'))).toBe('syntax');
    expect(classify(problem('error', 'expected a name, found `}`'))).toBe('syntax');
    expect(classify(problem('error', '`nonsense` is not a member of this Led'))).toBe('checker');
    expect(classify(problem('note', 'Led: not evaluated: a.b == c'))).toBe('solver');
    expect(classify(problem('warning', 'Led: part `x.r1`: not chosen: nothing in the catalog'))).toBe('solver');
    expect(classify(problem('note', 'Led: the current of part `a.b` is not known'))).toBe('solver');
    expect(classify(problem('warning', 'Led: track-width guideline: the net VCC needs 0.4 mm'))).toBe('guideline');
  });
});

describe('splitBoard and instancePath', () => {
  it('takes the board label off a build message', () => {
    expect(splitBoard('LedExample: not evaluated: x')).toEqual({ board: 'LedExample', rest: 'not evaluated: x' });
    expect(splitBoard('Sys/main: part not chosen')).toEqual({ board: 'Sys/main', rest: 'part not chosen' });
    expect(splitBoard('`x` is not a member of this Led')).toEqual({ board: null, rest: '`x` is not a member of this Led' });
  });

  it('finds the instance a message names, not a lone word in backticks', () => {
    expect(instancePath('the current of part `status_indicator.led` is not known')).toBe('status_indicator.led');
    expect(instancePath('`nonsense` is not a member of this Led')).toBeNull();
  });
});

describe('sortRows', () => {
  it('lists errors, then warnings, then notes; by file and line within one', () => {
    const list = sortRows(
      rows([
        problem('note', 'n', at('a.nsm', 1)),
        problem('warning', 'w2', at('b.nsm', 9)),
        problem('error', 'e-b', at('b.nsm', 3)),
        problem('warning', 'w1', at('a.nsm', 20)),
        problem('error', 'e-none'),
        problem('error', 'e-a', at('a.nsm', 30)),
        problem('error', 'e-a2', at('a.nsm', 4)),
      ]),
    );
    expect(list.map((r) => r.message)).toEqual(['e-a2', 'e-a', 'e-b', 'e-none', 'w1', 'w2', 'n']);
  });
});

describe('filterRows', () => {
  const view = { section: 'schematic' as const, file: null, board: 'Led' };
  const all = [
    ...rows([
      problem('error', 'Led: `x` is not a member', at('src/main.nsm', 3)),
      problem('warning', 'Other: part not chosen', at('src/other.nsm', 5)),
      problem('note', 'Led: not evaluated: a == b', at('src/main.nsm', 8)),
    ]),
    rowOfFinding({ id: '1', source: 'drc', severity: 'error', rule: 'clearance', message: 'tracks too close', x: 0, y: 0, area: { x: 0, y: 0, width: 3, height: 3 }, parts: ['R1'] } as PcbFinding, 'Led'),
  ];

  it('hides notes until asked for', () => {
    expect(filterRows(all, DEFAULT_FILTER, view)).toHaveLength(3);
    expect(filterRows(all, { ...DEFAULT_FILTER, notes: true }, view)).toHaveLength(4);
  });

  it('can show only errors', () => {
    const got = filterRows(all, { ...DEFAULT_FILTER, warnings: false }, view);
    expect(got.every((r) => r.severity === 'error')).toBe(true);
  });

  it('matches words in the message, the file and the stage', () => {
    expect(filterRows(all, { ...DEFAULT_FILTER, notes: true, text: 'other.nsm' }, view)).toHaveLength(1);
    expect(filterRows(all, { ...DEFAULT_FILTER, notes: true, text: 'solver main' }, view).map((r) => r.severity)).toEqual(['note']);
    expect(filterRows(all, { ...DEFAULT_FILTER, text: 'nothing like it' }, view)).toHaveLength(0);
  });

  it("'this view only' keeps what the section shows", () => {
    const only = { ...DEFAULT_FILTER, thisView: true, notes: true };
    // The schematic: the model's problems of its board, not the PCB's findings, not another board's.
    expect(filterRows(all, only, view).map((r) => r.message)).toEqual(['`x` is not a member', 'not evaluated: a == b']);
    // The PCB: its findings, and the project's problems of the board.
    expect(filterRows(all, only, { ...view, section: 'pcb' }).map((r) => r.origin)).toEqual(['checker', 'solver', 'rules']);
    // The source: the open file's.
    expect(filterRows(all, only, { section: 'source', file: 'src/other.nsm', board: 'Led' }).map((r) => r.message)).toEqual(['part not chosen']);
  });
});

describe('inScope', () => {
  const power = rowOfPowerIssue({ id: 'n1', level: 'bad', where: 'V3V3', text: 'can draw more than its supply gives' }, 'Led', null);
  it('puts a power issue in the power view and a PCB finding in the PCB view', () => {
    expect(inScope(power, { section: 'power', file: null, board: 'Led' })).toBe(true);
    expect(inScope(power, { section: 'schematic', file: null, board: 'Led' })).toBe(false);
    expect(power.severity).toBe('error');
    expect(power.origin).toBe('power');
  });
});

describe('groupRows', () => {
  const list = rows([
    problem('warning', 'w', at('b.nsm', 2)),
    problem('error', 'e', at('b.nsm', 1)),
    problem('error', 'e2', at('a.nsm', 1)),
    problem('error', 'nowhere'),
  ]);
  it('groups by file in the order of each file’s worst row', () => {
    const g = groupRows(list, 'file');
    expect(g.map((x) => x.title)).toEqual(['a.nsm', 'b.nsm', '(no file)']);
    expect(g[1].rows.map((r) => r.message)).toEqual(['e', 'w']);
  });
  it('groups by severity', () => {
    expect(groupRows(list, 'severity').map((x) => x.title)).toEqual(['Errors', 'Warnings']);
  });
  it('is one list with no grouping', () => {
    expect(groupRows(list, 'none')).toHaveLength(1);
  });
});

describe('nextRow (F8 and Shift+F8)', () => {
  const list: Row[] = sortRows(
    rows([problem('error', 'a', at('x.nsm', 5)), problem('error', 'b', at('x.nsm', 10, 4)), problem('warning', 'c', at('y.nsm', 2))]),
  );
  it('steps from the picked row and wraps at both ends', () => {
    expect(nextRow(list, list[0].id, null, 1)?.message).toBe('b');
    expect(nextRow(list, list[2].id, null, 1)?.message).toBe('a');
    expect(nextRow(list, list[0].id, null, -1)?.message).toBe('c');
  });
  it('with nothing picked, goes from the cursor', () => {
    expect(nextRow(list, null, { file: 'x.nsm', line: 6, column: 1 }, 1)?.message).toBe('b');
    expect(nextRow(list, null, { file: 'x.nsm', line: 6, column: 1 }, -1)?.message).toBe('a');
    // At the last one in the file, the first row of the list.
    expect(nextRow(list, null, { file: 'x.nsm', line: 11, column: 1 }, 1)?.message).toBe('a');
    // On a row's own position, "next" is the one after it, "previous" the one before.
    expect(nextRow(list, null, { file: 'x.nsm', line: 10, column: 4 }, 1)?.message).toBe('a');
    expect(nextRow(list, null, { file: 'x.nsm', line: 10, column: 4 }, -1)?.message).toBe('a');
  });
  it('with no cursor in a file with problems, the first or the last row', () => {
    expect(nextRow(list, null, { file: 'z.nsm', line: 1, column: 1 }, 1)?.message).toBe('a');
    expect(nextRow(list, null, null, -1)?.message).toBe('c');
  });
  it('has no next of nothing', () => {
    expect(nextRow([], null, null, 1)).toBeNull();
  });
  it('a picked row that went away is no picked row', () => {
    expect(nextRow(list, 'gone', null, 1)?.message).toBe('a');
  });
});

describe('steps', () => {
  it('leaves the notes out of the stepping unless they are all there is', () => {
    const mixed = rows([problem('note', 'n', at('a.nsm', 1)), problem('error', 'e', at('a.nsm', 2))]);
    expect(steps(mixed, 'none').map((r) => r.message)).toEqual(['e']);
    expect(steps(mixed.filter((r) => r.severity === 'note'), 'none').map((r) => r.message)).toEqual(['n']);
  });
});

describe('counts and summary', () => {
  it('counts each severity and says them plainly', () => {
    const c = countRows(rows([problem('error', 'a'), problem('error', 'b'), problem('warning', 'c'), problem('note', 'd')]));
    expect(c).toEqual({ error: 2, warning: 1, note: 1 });
    expect(summaryText(c)).toBe('2 errors, 1 warning');
    expect(summaryText({ error: 1, warning: 0, note: 5 })).toBe('1 error');
    expect(summaryText({ error: 0, warning: 0, note: 5 })).toBe('No problems');
  });
});

describe('row ids', () => {
  it('are the same for the same problem read again', () => {
    const p = problem('error', 'x', at('a.nsm', 3));
    expect(rowOfProblem(p, 0).id).toBe(rowOfProblem({ ...p }, 0).id);
  });
});
