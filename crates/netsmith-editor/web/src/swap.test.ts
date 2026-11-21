import { describe, expect, it } from 'vitest';

import type { SwapAlternative } from './api';
import { editLabel } from './bomview';

const at = { file: 'src/main.nsm', line: 3, column: 5, endLine: 3, endColumn: 5 };
const alt = (edit: SwapAlternative['edit']): SwapAlternative => ({
  name: '240Ω 1% 0402 C25094',
  line: '240Ω 1% 0402 C25094 (-3.6%, Extended)',
  sku: 'C25094',
  mpn: null,
  check: 'passes',
  why: null,
  value: '240ohm +/- 1%',
  edit,
});

describe('the edit a swap writes', () => {
  it('says the text it replaces and its new text', () => {
    expect(editLabel(alt({ at, old: '249ohm +/- 1%', new: '240ohm +/- 1%' }))).toBe('249ohm +/- 1% → 240ohm +/- 1%');
  });
  it('says the statement it adds where nothing states the value', () => {
    expect(editLabel(alt({ at, old: '{}', new: '{ .resistance &= 240ohm +/- 1% }' }))).toBe('{} → { .resistance &= 240ohm +/- 1% }');
    expect(editLabel(alt({ at, old: '', new: '\n    .resistance &= 240ohm +/- 1%' }))).toBe('adds .resistance &= 240ohm +/- 1%');
    expect(editLabel(alt({ at, old: '', new: ' { .resistance &= 240ohm +/- 1% }' }))).toBe('adds .resistance &= 240ohm +/- 1%');
  });
  it('is empty when the swap cannot write the row', () => {
    expect(editLabel(alt(null))).toBe('');
  });
});
