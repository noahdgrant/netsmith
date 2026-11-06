import { describe, expect, it } from 'vitest';

import { ampsText } from '../power/format';
import {
  boardMarks,
  boardStatusText,
  bomKey,
  bomMarks,
  bomRows,
  changeCount,
  costDelta,
  counts,
  filterRows,
  groupRows,
  headroomTrend,
  marksOf,
  markTargets,
  otherBoardTargets,
  moduleRows,
  netRows,
  onlySourceChanged,
  partRows,
  problemRows,
  railRows,
  removedOn,
  renumberedRows,
  rowsOf,
  sheetOfRemoved,
  stageIndex,
  stageText,
  tiles,
} from './model';
import { board, changed } from './testdata';

const c = changed();
const power = c.boards[1];
const led = c.boards[0];

describe('the rows of the parts tab', () => {
  it('an added part has its new value, its designator on the head side and where it was declared', () => {
    const [r] = partRows(power).filter((x) => x.status === 'added');
    expect(r).toMatchObject({ title: 'stage.bulk', type: 'Capacitor', after: '47u', desHead: 'C1', module: 'stage', board: 'TwoBoards/power', category: 'parts' });
    expect(r.before).toBeUndefined();
    expect(r.head?.line).toBe(24);
    expect(r.base).toBeUndefined();
    expect(r.select).toEqual({ kind: 'part', key: 'stage.bulk' });
  });
  it('a removed part has its old value and designator, the line it had at the ref, and nothing to select', () => {
    const [r] = partRows(power).filter((x) => x.status === 'removed');
    expect(r).toMatchObject({ title: 'stage.snubber', before: '100n', desBase: 'C9' });
    expect(r.base?.line).toBe(30);
    expect(r.head).toBeUndefined();
    expect(r.select).toBeUndefined();
  });
  it('a changed part shows the value first, with the other fields for the expanded row', () => {
    const [r] = partRows(led);
    expect(r).toMatchObject({ status: 'changed', before: '100', after: '150', field: 'value', desBase: 'R1', desHead: 'R1' });
    expect(r.fields.map((f) => f.field)).toEqual(['value', 'lcsc']);
    expect(r.base?.line).toBe(21);
    expect(r.head?.line).toBe(21);
  });
  it('a part that moved is moved, with the path it had', () => {
    const r = partRows(power).find((x) => x.status === 'moved')!;
    expect(r).toMatchObject({ title: 'stage.regulator', baseTitle: 'ldo.regulator', before: 'ldo.regulator', after: 'stage.regulator', field: 'path' });
  });
  it('a renumbered part shows its designators on each side and is listed apart', () => {
    const rows = renumberedRows(power);
    expect(rows.map((r) => `${r.before}>${r.after}`)).toEqual(['C1>C2', 'C2>C3']);
    expect(rows.every((r) => r.status === 'renumbered' && r.field === 'designator')).toBe(true);
    expect(partRows(power).some((r) => r.status === 'renumbered')).toBe(false);
  });
});

describe('the rows of the nets tab', () => {
  const rows = netRows(power);
  it('a renamed net shows both names; a changed one lists the pins that joined and left, with each side’s designator', () => {
    expect(rows.find((r) => r.status === 'renamed')).toMatchObject({ title: 'VOUT', baseTitle: '3V3', before: '3V3', after: 'VOUT' });
    const gnd = rows.find((r) => r.title === 'GND')!;
    expect(gnd.status).toBe('changed');
    expect(gnd.pins.map((p) => `${p.sign}${p.designator}.${p.pin}`)).toEqual(['+C1.2', '-C9.2']);
    expect(gnd.before).toBe('1 left');
    expect(gnd.after).toBe('1 joined');
    expect(gnd.select).toEqual({ kind: 'net', key: 'GND' });
    expect(gnd.module).toBe('stage');
  });
  it('an added and a removed net list all their pins', () => {
    const b = board('X', { nets: { added: [{ name: 'N', named: true, pins: [{ part: 'a.b', designator: 'R1', pin: '1' }, { part: 'a.c', designator: 'R2', pin: '2' }] }], removed: [{ name: 'M', named: true, pins: [{ part: 'a.b', designator: 'R1', pin: '2' }] }], renamed: [], changed: [], unchanged: 0 } });
    const r = netRows(b);
    expect(r[0]).toMatchObject({ status: 'added', after: '2 pins' });
    expect(r[0].pins).toHaveLength(2);
    expect(r[1]).toMatchObject({ status: 'removed', before: '1 pin' });
    expect(r[1].select).toBeUndefined();
  });
});

describe('the rows of the rails tab', () => {
  const rows = railRows(power);
  it('a rail that lost headroom says from what to what, and that it shrank', () => {
    const r = rows.find((x) => x.title === 'VOUT')!;
    expect(r).toMatchObject({ field: 'headroom', before: '50 mA', after: '20 mA', trend: 'shrank', baseTitle: '3V3' });
    expect(r.fields.map((f) => f.field)).toEqual(['headroom', 'name', 'demand']);
    expect(r.fields[2]).toMatchObject({ old: '0A..50mA', new: '0A..80mA' });
  });
  it('one that gained it grew; one whose budget changed shows the budget', () => {
    expect(rows.find((x) => x.title === 'VIN')).toMatchObject({ before: '20 mA', after: '50 mA', trend: 'grew' });
    const aux = rows.find((x) => x.title === 'AUX')!;
    expect(aux).toMatchObject({ field: 'budget', before: 'within', after: 'over' });
    expect(aux.trend).toBeUndefined();
  });
  it('compares headroom to the nanoampere, and says nothing when a side does not know it', () => {
    expect(headroomTrend(0.05, 0.02)).toBe('shrank');
    expect(headroomTrend(0.02, 0.05)).toBe('grew');
    expect(headroomTrend(0.05, 0.05 + 4e-10)).toBeNull();
    expect(headroomTrend(undefined, 0.02)).toBeNull();
    expect(ampsText(0.05)).toBe('50 mA');
  });
});

describe('the rows of the bill of materials', () => {
  it('lines that were added, removed and changed, with quantities and prices on each side', () => {
    const rows = bomRows(led);
    expect(rows.map((r) => r.status)).toEqual(['added', 'removed']);
    expect(rows[0]).toMatchObject({ title: '150 Resistor', after: 'qty 1', desHead: 'R1' });
    expect(rows[1]).toMatchObject({ title: '100 Resistor', before: 'qty 1', desBase: 'R1' });
    const b = board('X', { bom: { added: [], removed: [], unchanged: 0, changed: [{ kind: 'Resistor', value: '10k', footprint: '0603', handSolder: false, quantityBase: 2, quantityHead: 3, designatorsBase: ['R1', 'R2'], designatorsHead: ['R1', 'R2', 'R3'], unitUsdBase: 0.001, unitUsdHead: 0.002 }] } });
    const [r] = bomRows(b);
    expect(r).toMatchObject({ status: 'changed', before: '2', after: '3', field: 'quantity' });
    expect(r.fields.map((f) => f.field)).toEqual(['quantity', 'unit price']);
  });
  it('is empty when a side has no bill', () => {
    expect(bomRows(board('X', { bom: undefined, bomError: 'no' }))).toEqual([]);
  });
  it('a line is identified as the server groups it', () => {
    expect(bomKey({ kind: 'R', value: '1k', footprint: '0603', lcsc: 'C1', mpn: null, handSolder: false })).toBe('R|1k|0603|C1||');
    expect(bomKey({ kind: 'R', value: '1k', handSolder: true })).toBe('R|1k||||hand');
  });
});

describe('modules, interfaces and problems', () => {
  it('modules and ports added, removed and changed', () => {
    const rows = moduleRows(power);
    expect(rows.map((r) => `${r.kind}:${r.status}:${r.title}`)).toEqual(['module:added:stage', 'module:changed:filter']);
    expect(rows[1]).toMatchObject({ before: 'Pi', after: 'Lc' });
    expect(rows[0].select).toEqual({ kind: 'module', key: 'stage' });
  });
  it('a new problem has the head’s line, a resolved one the ref’s', () => {
    const rows = problemRows('TwoBoards/power', power.problems);
    expect(rows[0]).toMatchObject({ status: 'new', severity: 'warning', title: 'stage.bulk: no LCSC number' });
    expect(rows[0].head?.line).toBe(24);
    expect(rows[0].base).toBeUndefined();
    expect(rows[0].select).toEqual({ kind: 'part', key: 'stage.bulk' });
    expect(rows[1]).toMatchObject({ status: 'resolved', severity: 'error' });
    expect(rows[1].base?.line).toBe(30);
    expect(rows[1].head).toBeUndefined();
  });
});

describe('the rows of a scope', () => {
  it('are of one board, or of all of them with the board named in each row', () => {
    expect(rowsOf(c, 'TwoBoards/led').parts).toHaveLength(1);
    expect(rowsOf(c, 'all').parts).toHaveLength(4);
    expect(new Set(rowsOf(c, 'all').parts.map((r) => r.board))).toEqual(new Set(['TwoBoards/led', 'TwoBoards/power']));
  });
  it('count each tab, and the source files', () => {
    expect(counts(c, 'TwoBoards/power')).toEqual({ parts: 3, nets: 3, rails: 3, bom: 0, modules: 2, problems: 2, source: 1 });
  });
  it('the chip counts the changes to the circuit, not its bill, its problems or its sources', () => {
    // led: 1 part, 0 nets, 0 rails; power: 3 parts, 3 nets, 3 rails, 2 modules
    expect(changeCount(c)).toBe(1 + 3 + 3 + 3 + 2);
    expect(changeCount(null)).toBe(0);
    expect(changeCount({ ...c, error: 'does not analyse' })).toBe(0);
  });
  it('knows when only the source differs', () => {
    const same = { ...c, changed: false, boards: [board('B')] };
    expect(onlySourceChanged(same)).toBe(true);
    expect(onlySourceChanged({ ...same, source: { filesChanged: 0, added: 0, removed: 0, files: [] } })).toBe(false);
    expect(onlySourceChanged(c)).toBe(false);
  });
  it('says what each board has changed', () => {
    expect(boardStatusText(led)).toBe('1 change');
    expect(boardStatusText(power)).toBe('11 changes');
    expect(boardStatusText(board('B'))).toBe('unchanged');
    expect(boardStatusText(board('B', { status: 'added' }))).toBe('new board');
    expect(boardStatusText(board('B', { status: 'removed' }))).toBe('removed');
  });
});

describe('the tiles', () => {
  const t = (scope: string) => Object.fromEntries(tiles(c, scope).map((x) => [x.id, x]));
  it('parts, nets and rails count +, - and ~ for the boards in scope', () => {
    const p = t('TwoBoards/power');
    expect(p.parts).toMatchObject({ value: '3', sub: '2 renumbered' });
    expect(p.parts.chips).toEqual([{ sign: '+', n: 1, label: 'added' }, { sign: '-', n: 1, label: 'removed' }, { sign: '~', n: 1, label: 'changed' }]);
    expect(p.nets.chips).toEqual([{ sign: '~', n: 3, label: 'changed' }]);
    expect(p.rails).toMatchObject({ value: '3', sub: '1 headroom shrank', tone: 'warn' });
    expect(t('all').parts.value).toBe('4');
  });
  it('the BOM tile has the cost delta of the bill, and says unknown when a price is', () => {
    expect(t('TwoBoards/led').bom).toMatchObject({ value: '+$0.0010', tone: 'warn' });
    expect(t('TwoBoards/power').bom).toMatchObject({ value: 'no change', sub: 'no line differs' });
    // a line differs and no side has a price for it
    const noPrice = { ...c, boards: [board('B', { bom: { added: [{ kind: 'R', value: '1k', handSolder: false, quantity: 1, designators: ['R1'] }], removed: [], changed: [], unchanged: 1 } })] };
    expect(tiles(noPrice, 'all').find((x) => x.id === 'bom')!.value).toBe('unknown');
    // a bill that could not be made
    expect(tiles({ ...c, boards: [board('B', { bom: undefined, bomError: 'x' })] }, 'all').find((x) => x.id === 'bom')!.value).toBe('unknown');
  });
  it('the problems tile is red for a new problem and green for one that was resolved', () => {
    expect(t('TwoBoards/power').problems).toMatchObject({ value: '1 new', sub: '1 resolved', tone: 'bad' });
    expect(t('TwoBoards/led').problems).toMatchObject({ value: 'None', tone: 'neutral' });
    const resolved = { ...c, boards: [board('B', { problems: { new: [], resolved: power.problems.resolved, unchanged: 0 } })] };
    expect(tiles(resolved, 'all').find((x) => x.id === 'problems')).toMatchObject({ value: '0 new', sub: '1 resolved', tone: 'ok' });
  });
  it('writes a cost delta as a person would', () => {
    expect(costDelta(0.012)).toBe('+$0.012');
    expect(costDelta(-0.3)).toBe('-$0.300');
    expect(costDelta(0)).toBe('no change');
    expect(costDelta(undefined)).toBe('unknown');
  });
});

describe('grouping and filtering', () => {
  const rows = rowsOf(c, 'all').parts;
  it('flat is one list; by module has the board’s own parts first; by change orders the kinds of change', () => {
    expect(groupRows(rows, 'none')).toHaveLength(1);
    expect(groupRows([], 'none')).toEqual([]);
    const byModule = groupRows(rows, 'module');
    expect(byModule.map((g) => g.key)).toEqual(['', 'ldo'.length ? 'stage' : '']);
    expect(byModule[0].label).toBe('On the board');
    expect(byModule[1].rows.map((r) => r.title).sort()).toEqual(['stage.bulk', 'stage.regulator', 'stage.snubber']);
    const byChange = groupRows(rows, 'change');
    expect(byChange.map((g) => g.label)).toEqual(['Added', 'Removed', 'Changed', 'Moved']);
  });
  it('a filter needs every word, in any field, including the old value and the status', () => {
    expect(filterRows(rows, '').length).toBe(rows.length);
    expect(filterRows(rows, '100n').map((r) => r.title)).toEqual(['stage.snubber']);
    expect(filterRows(rows, 'capacitor added').map((r) => r.title)).toEqual(['stage.bulk']);
    expect(filterRows(rows, 'R1').map((r) => r.title)).toEqual(['r1']);
    expect(filterRows(rows, 'ldo.regulator').map((r) => r.title)).toEqual(['stage.regulator']);
    expect(filterRows(rows, 'zzz')).toEqual([]);
  });
});

describe('the stage of a comparison in progress', () => {
  it('says the stage in words, or what the server said, and never a percentage', () => {
    expect(stageText(null)).toBe('Starting');
    expect(stageText({ stage: 'extract', message: '' })).toBe('Reading the files at the ref from git');
    expect(stageText({ stage: 'solve', message: 'solving TwoBoards, 1 of 1', done: 1, total: 1 })).toBe('Solving TwoBoards, 1 of 1');
    expect(stageText({ stage: 'diff', message: '' })).toMatch(/Comparing/);
    expect(stageText({ message: '' })).toBe('Working');
    for (const s of ['extract', 'check', 'solve', 'lower', 'draw', 'modules', 'diff']) expect(stageText({ stage: s, message: '' })).not.toMatch(/%/);
  });
  it('knows where a stage is in the order the server goes through them', () => {
    expect(['extract', 'check', 'solve', 'lower', 'draw', 'modules', 'diff'].map(stageIndex)).toEqual([0, 1, 2, 3, 4, 5, 6]);
    expect(stageIndex(undefined)).toBe(-1);
    expect(stageIndex('other')).toBe(-1);
  });
});

describe('the marks on the other views', () => {
  const m = marksOf(power);
  it('parts by their path on the head side, and the modules above a part that differs', () => {
    expect([...m.parts]).toEqual([['stage.bulk', 'added'], ['stage.regulator', 'changed'], ['stage.input_cap', 'renumbered'], ['stage.output_cap', 'renumbered']]);
    expect(m.modules.get('stage')).toBe('added');
    expect(m.modules.get('filter')).toBe('changed');
    expect(marksOf(led).modules.size).toBe(0);
  });
  it('removed parts are kept for the inspector, with their values and where they were', () => {
    expect(m.removedParts).toEqual([{ path: 'stage.snubber', designator: 'C9', kind: 'Capacitor', value: '100n', module: 'stage', at: expect.objectContaining({ line: 30 }) }]);
  });
  it('nets by their name on the head side; a removed net is not in the drawing', () => {
    expect([...m.nets]).toEqual([['VOUT', 'changed'], ['VIN', 'changed'], ['GND', 'changed']]);
    expect(m.removedNets).toEqual([]);
  });
  it('rails that lost headroom carry the badge, the others only the mark', () => {
    expect(m.rails.get('VOUT')).toEqual({ mark: 'changed', trend: 'shrank', delta: '50 mA → 20 mA' });
    expect(m.rails.get('VIN')).toMatchObject({ trend: 'grew', delta: '20 mA → 50 mA' });
    expect(m.rails.get('AUX')).toEqual({ mark: 'changed', trend: undefined, delta: undefined });
  });
  it('has nothing for a board the comparison does not have', () => {
    const none = marksOf(undefined);
    expect(none.parts.size + none.nets.size + none.rails.size + none.removedParts.length).toBe(0);
  });
  it('assigns each mark a class and a selector: the schematic by designator, the cards by `data-hv`, the board by `data-path`', () => {
    const t = markTargets(m, (path) => ({ 'stage.bulk': 'C1', 'stage.regulator': 'U1' })[path]);
    const has = (selector: string, cls: string) => t.some((x) => x.selector === selector && x.cls === cls);
    expect(has('[data-ref="C1"]:not(.pin)', 'cmp-added')).toBe(true);
    expect(has('[data-ref="U1"]:not(.pin)', 'cmp-changed')).toBe(true);
    expect(has('[data-hv="part:stage.bulk"]', 'cmp-added')).toBe(true);
    expect(has('[data-path="stage.bulk"]', 'cmp-added')).toBe(true);
    // a part with no designator on the head (it has no drawing) is still marked where the path says it
    expect(has('[data-hv="part:stage.input_cap"]', 'cmp-renumbered')).toBe(true);
    expect(t.some((x) => x.selector.includes('stage.input_cap') && x.selector.startsWith('[data-ref'))).toBe(false);
    expect(has('[data-net="VOUT"]:not(.pin)', 'cmp-net-changed')).toBe(true);
    expect(has('[data-hv="net:GND"]', 'cmp-net-changed')).toBe(true);
    expect(has('[data-hv="module:stage"]', 'cmp-added')).toBe(true);
    const rail = t.find((x) => x.selector === '.pt-rail[data-hv="net:VOUT"]:not([data-cmp-board])')!;
    expect(rail).toMatchObject({ cls: 'cmp-changed cmp-shrank', delta: '50 mA → 20 mA' });
    expect(t.find((x) => x.selector === '.pt-rail[data-hv="net:VIN"]:not([data-cmp-board])')!.cls).toBe('cmp-changed cmp-grew');
  });
  it('tells the rails of a system’s boards apart by the board a card says it is on', () => {
    const front = markTargets(m, () => undefined, 'power').filter((x) => x.selector.startsWith('.pt-rail'));
    expect(front.map((x) => x.selector)).toContain('.pt-rail[data-hv="net:VOUT"][data-cmp-board="power"]');
    expect(front.map((x) => x.selector)).toContain('.pt-rail[data-hv="net:VOUT"]:not([data-cmp-board])');
    expect(otherBoardTargets(marksOf(led), 'led')).toEqual([]);
    const ledRails = marksOf(board('Sys/led', { rails: { added: [], removed: [], unchanged: 0, changed: [{ net: '3V3', headroomBase: 0.01, headroomHead: 0.02 }] } }));
    expect(otherBoardTargets(ledRails, 'led')).toEqual([{ selector: '.pt-rail[data-hv="net:3V3"][data-cmp-board="led"]', cls: 'cmp-changed cmp-grew', delta: '10 mA → 20 mA' }]);
  });
  it('quotes a name that has a quote in it', () => {
    const odd = marksOf(board('B', { nets: { added: [{ name: 'a"b', named: true, pins: [] }], removed: [], renamed: [], changed: [], unchanged: 0 } }));
    expect(markTargets(odd, () => undefined).some((x) => x.selector === '[data-net="a\\"b"]:not(.pin)')).toBe(true);
  });
  it('marks the boards of a system by their own name', () => {
    expect([...boardMarks(c)]).toEqual([['led', 'changed'], ['power', 'changed']]);
    expect(boardMarks(null).size).toBe(0);
    expect(boardMarks({ ...c, boards: [board('Sys/a'), board('Sys/b', { status: 'added' })] }).get('b')).toBe('added');
  });
});

describe('removed parts on a sheet', () => {
  const parts = [{ path: 'stage.regulator', refdes: 'U1' }, { path: 'stage.bulk', refdes: 'C1' }, { path: 'j1', refdes: 'J1' }];
  const sheets = new Map([['U1', 'stage'], ['C1', 'stage'], ['J1', 'board']]);
  it('are on the sheet of the parts that share their module', () => {
    expect(sheetOfRemoved('stage', parts, sheets)).toBe('stage');
    expect(sheetOfRemoved('', parts, sheets)).toBe('board');
    // a module below one that has parts: the sheet of what is below, then above
    expect(sheetOfRemoved('stage.inner', parts, sheets)).toBe('stage');
    expect(sheetOfRemoved('gone', parts, sheets)).toBe('board');
    expect(sheetOfRemoved('', [], sheets)).toBeUndefined();
  });
  it('are all listed in the overview, and by sheet elsewhere', () => {
    const removed = marksOf(power).removedParts;
    expect(removedOn(removed, parts, sheets, '*', '*')).toHaveLength(1);
    expect(removedOn(removed, parts, sheets, 'stage', '*')).toHaveLength(1);
    expect(removedOn(removed, parts, sheets, 'board', '*')).toHaveLength(0);
  });
});

describe('the bill of materials marks', () => {
  const d = bomMarks(led.bom);
  it('mark a line that is new, with its quantity and price, and keep the removed lines to show at the end', () => {
    const key = bomKey({ kind: 'Resistor', value: '150', footprint: '0603', lcsc: 'C22775', handSolder: false });
    expect(d.byKey.get(key)).toMatchObject({ status: 'added', quantityHead: 1, unitHead: 0.002 });
    expect(d.byKey.get(key)!.deltaUsd).toBeCloseTo(0.002);
    expect(d.removed.map((l) => l.value)).toEqual(['100']);
  });
  it('a line that changed has the quantity on each side and the change of price for the whole line', () => {
    const m = bomMarks({ added: [], removed: [], unchanged: 0, changed: [{ kind: 'R', value: '1k', handSolder: false, quantityBase: 2, quantityHead: 3, designatorsBase: [], designatorsHead: [], unitUsdBase: 0.001, unitUsdHead: 0.002 }] });
    const x = m.byKey.get(bomKey({ kind: 'R', value: '1k' }))!;
    expect(x).toMatchObject({ status: 'changed', quantityBase: 2, quantityHead: 3 });
    expect(x.deltaUsd).toBeCloseTo(0.006 - 0.002);
  });
  it('is empty without a bill', () => {
    expect(bomMarks(undefined).byKey.size).toBe(0);
  });
});
