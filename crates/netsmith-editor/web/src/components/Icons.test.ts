import { describe, expect, it } from 'vitest';

import { P, severityWords } from './Icons';

describe('severityWords (the name of a mark that counts)', () => {
  it('says each severity with its own plural', () => {
    expect(severityWords({ error: 1, warning: 2, note: 3 })).toBe('1 error, 2 warnings, 3 notes');
    expect(severityWords({ error: 0, warning: 1 })).toBe('0 errors, 1 warning, 0 notes');
  });
  it('leaves out what there is none of when it is quiet: a mark for two warnings does not say "0 errors"', () => {
    expect(severityWords({ error: 0, warning: 2 }, true)).toBe('2 warnings');
    expect(severityWords({ error: 1, warning: 0, note: 0 }, true)).toBe('1 error');
    expect(severityWords({}, true)).toBe('');
  });
});

describe('the icon set', () => {
  it('has a drawing for each severity (the recipe `warning-marks` reads the triangle in every place a warning is shown)', () => {
    for (const name of ['error', 'warning', 'note']) expect(P[name], name).toBeTruthy();
  });
});
