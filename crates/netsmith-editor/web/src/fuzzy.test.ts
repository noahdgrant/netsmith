import { describe, expect, it } from 'vitest';

import { fuzzyMatch, matchFields, pieces } from './fuzzy';

describe('fuzzyMatch', () => {
  it('needs the letters in order', () => {
    expect(fuzzyMatch('abc', 'a_b_c')).not.toBeNull();
    expect(fuzzyMatch('acb', 'abc')).toBeNull();
    expect(fuzzyMatch('x', 'abc')).toBeNull();
    expect(fuzzyMatch('abcd', 'abc')).toBeNull();
  });

  it('ignores case and matches everything with an empty query', () => {
    expect(fuzzyMatch('LDO', 'ldo_stage')).not.toBeNull();
    expect(fuzzyMatch('', 'anything')).toEqual({ score: 0, indices: [] });
  });

  it('gives the positions of the letters', () => {
    expect(fuzzyMatch('ls', 'ldo_stage')!.indices).toEqual([0, 4]);
    expect(fuzzyMatch('stage', 'ldo_stage')!.indices).toEqual([4, 5, 6, 7, 8]);
  });

  it('ranks exact over prefix over word start over scattered', () => {
    const exact = fuzzyMatch('c25', 'C25')!.score;
    const prefix = fuzzyMatch('c25', 'C250')!.score;
    const word = fuzzyMatch('c25', 'bank.C25')!.score;
    const scattered = fuzzyMatch('c25', 'cap_x_2_y_5')!.score;
    expect(exact).toBeGreaterThan(prefix);
    expect(prefix).toBeGreaterThan(word);
    expect(word).toBeGreaterThan(scattered);
  });

  it('prefers the start of a word to the middle of one', () => {
    expect(fuzzyMatch('st', 'ldo_stage')!.score).toBeGreaterThan(fuzzyMatch('st', 'lcdost')!.score);
    expect(fuzzyMatch('ps', 'PullStage')!.score).toBeGreaterThan(fuzzyMatch('ps', 'apples')!.score);
  });

  it('prefers a run of letters to letters spread out', () => {
    expect(fuzzyMatch('vcc', 'vcc_3v3')!.score).toBeGreaterThan(fuzzyMatch('vcc', 'v_c_c_x')!.score);
  });

  it('finds the best layout, not the first', () => {
    // `ab` could take the first a, but the run `ab` at the end is better.
    expect(fuzzyMatch('ab', 'a_x_ab')!.indices).toEqual([4, 5]);
  });

  it('prefers the shorter text for the same letters', () => {
    expect(fuzzyMatch('led', 'led')!.score).toBeGreaterThan(fuzzyMatch('led', 'ledger')!.score);
  });
});

describe('matchFields', () => {
  const fields = [
    { name: 'label', text: 'C25', weight: 3 },
    { name: 'sub', text: '100n' },
    { name: 'detail', text: 'light1.bypass' },
  ];
  it('matches a word against any field', () => {
    expect(matchFields('c25', fields)).not.toBeNull();
    expect(matchFields('100n', fields)!.marks.sub).toEqual([0, 1, 2, 3]);
    expect(matchFields('bypass', fields)!.marks.detail).toEqual([7, 8, 9, 10, 11, 12]);
  });
  it('needs every word to match', () => {
    expect(matchFields('c25 100n', fields)).not.toBeNull();
    expect(matchFields('c25 zzz', fields)).toBeNull();
  });
  it('weighs the field', () => {
    const heavy = matchFields('c25', [{ name: 'a', text: 'C25', weight: 3 }])!.score;
    const light = matchFields('c25', [{ name: 'a', text: 'C25', weight: 1 }])!.score;
    expect(heavy).toBeCloseTo(light * 3);
  });
  it('matches everything with an empty query', () => {
    expect(matchFields('  ', fields)).toEqual({ score: 0, marks: {} });
  });
});

describe('pieces', () => {
  it('cuts the text at the marked letters', () => {
    expect(pieces('abcd', [1, 2])).toEqual([
      { text: 'a', hit: false },
      { text: 'bc', hit: true },
      { text: 'd', hit: false },
    ]);
    expect(pieces('abc', undefined)).toEqual([{ text: 'abc', hit: false }]);
    expect(pieces('abc', [0])).toEqual([
      { text: 'a', hit: true },
      { text: 'bc', hit: false },
    ]);
  });
});
