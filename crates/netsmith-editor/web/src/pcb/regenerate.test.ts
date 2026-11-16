import { describe, expect, it } from 'vitest';

import { inModule, moduleOf, moduleOfAll, totalLine } from './regenerate';

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
