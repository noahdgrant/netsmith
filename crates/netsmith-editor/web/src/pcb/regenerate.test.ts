import { describe, expect, it } from 'vitest';

import { inModule, moduleOf, moduleOfAll, moduleRows, totalLine } from './regenerate';

describe('the module of a selection', () => {
  it('is the path less its last word', () => {
    expect(moduleOf('a.led')).toBe('a');
    expect(moduleOf('stage.bank[2]')).toBe('stage.bank');
    expect(moduleOf('channels[1].r')).toBe('channels[1]');
    expect(moduleOf('r1')).toBeNull();
  });

  it('reads a module by whole words, as the server does', () => {
    expect(inModule('stage.input_cap', 'stage')).toBe(true);
    expect(inModule('stage2.r1', 'stage')).toBe(false);
    expect(inModule('bank[0]', 'bank')).toBe(true);
    expect(inModule('a', '')).toBe(false);
  });

  it('is the innermost module that holds every part picked', () => {
    expect(moduleOfAll(['a.led', 'a.r'])).toBe('a');
    expect(moduleOfAll(['top.a.led', 'top.b.r'])).toBe('top');
    expect(moduleOfAll(['a.led', 'b.r'])).toBeNull();
    expect(moduleOfAll(['r1'])).toBeNull();
    expect(moduleOfAll([])).toBeNull();
  });
});

describe('a total before and after', () => {
  it('says the change with its sign', () => {
    expect(totalLine('ratsnest', 34.41, 31.73, ' mm')).toBe('ratsnest 34.41 → 31.73 mm (−2.68)');
    expect(totalLine('crossings', 1, 2, '')).toBe('crossings 1 → 2 (+1)');
    expect(totalLine('crossings', 1, 1, '')).toBe('crossings 1 → 1 (no change)');
  });
});

describe('the tree of a board\'s modules', () => {
  it('nests every module the parts are in, with the parts in each', () => {
    const rows = moduleRows(['r1', 'a.led', 'a.r', 'stage.bank[0]', 'stage.bank[1]', 'stage.in.c', 'b.led']);
    expect(rows.map((r) => [r.path, r.label, r.depth, r.parts])).toEqual([
      ['a', 'a', 0, 2],
      ['b', 'b', 0, 1],
      ['stage', 'stage', 0, 3],
      ['stage.bank', 'bank', 1, 2],
      ['stage.in', 'in', 1, 1],
    ]);
  });

  it('puts the elements of an array of modules under the array, in their order', () => {
    const paths = ['ch[10].r', 'ch[2].r', 'ch[2].led'];
    expect(moduleRows(paths).map((r) => [r.label, r.depth, r.parts])).toEqual([
      ['ch', 0, 3],
      ['[2]', 1, 2],
      ['[10]', 1, 1],
    ]);
  });

  it('is empty for a board with parts of its own only', () => {
    expect(moduleRows(['r1', 'c1'])).toEqual([]);
  });
});
