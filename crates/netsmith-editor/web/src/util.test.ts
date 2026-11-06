import { describe, expect, it } from 'vitest';

import { ageLabel, spaceUnits } from './util';

describe('ageLabel', () => {
  it('says how long ago a price was checked', () => {
    expect(ageLabel(0)).toBe('today');
    expect(ageLabel(1)).toBe('1 day ago');
    expect(ageLabel(91)).toBe('91 days ago');
    expect(ageLabel(-3)).toBe('in the future');
    expect(ageLabel(null)).toBe('unknown');
  });
});

describe('spaceUnits', () => {
  it('puts a thin space between the number and the unit and round the dots of a range', () => {
    expect(spaceUnits('3.135V..3.465V')).toBe('3.135 V .. 3.465 V');
    expect(spaceUnits('1.72uF..2.916uF')).toBe('1.72 uF .. 2.916 uF');
    expect(spaceUnits('-50mA..0A')).toBe('-50 mA .. 0 A');
  });
  it('does the same for a single value, a tolerance and a rating', () => {
    expect(spaceUnits('3.3V')).toBe('3.3 V');
    expect(spaceUnits('+/- 20%')).toBe('+/- 20 %');
    expect(spaceUnits('16V')).toBe('16 V');
  });
  it('leaves an exponent alone', () => {
    expect(spaceUnits('1e-3A')).toBe('1e-3 A');
    expect(spaceUnits('2.5e3V')).toBe('2.5e3 V');
  });
  it('gives the same answer for text that has its spaces already', () => {
    expect(spaceUnits('3.135 V .. 3.465 V')).toBe(spaceUnits('3.135V..3.465V'));
    expect(spaceUnits(spaceUnits('3.135V..3.465V'))).toBe(spaceUnits('3.135V..3.465V'));
  });
  it('leaves text without a quantity alone', () => {
    expect(spaceUnits('')).toBe('');
    expect(spaceUnits('GND')).toBe('GND');
  });
});
