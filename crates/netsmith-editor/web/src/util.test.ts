import { describe, expect, it } from 'vitest';

import { ageLabel } from './util';

describe('ageLabel', () => {
  it('says how long ago a price was checked', () => {
    expect(ageLabel(0)).toBe('today');
    expect(ageLabel(1)).toBe('1 day ago');
    expect(ageLabel(91)).toBe('91 days ago');
    expect(ageLabel(-3)).toBe('in the future');
    expect(ageLabel(null)).toBe('unknown');
  });
});
