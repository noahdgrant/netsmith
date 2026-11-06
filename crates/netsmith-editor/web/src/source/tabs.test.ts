import { describe, expect, it } from 'vitest';

import { dropBefore, reorder } from './tabs';

const L = ['a', 'b', 'c', 'd'];
describe('reorder', () => {
  it('moves a tab before another', () => {
    expect(reorder(L, 'd', 'b')).toEqual(['a', 'd', 'b', 'c']);
    expect(reorder(L, 'a', 'c')).toEqual(['b', 'a', 'c', 'd']);
  });
  it('moves a tab to the end', () => {
    expect(reorder(L, 'a', null)).toEqual(['b', 'c', 'd', 'a']);
  });
  it('changes nothing for the same place, an unknown tab or a drop where it already is', () => {
    expect(reorder(L, 'b', 'b')).toBe(L);
    expect(reorder(L, 'x', 'a')).toBe(L);
    expect(reorder(L, 'b', 'c')).toBe(L);
    expect(reorder(L, 'd', null)).toBe(L);
    expect(reorder(L, 'a', 'zzz')).toBe(L);
  });
});

describe('dropBefore', () => {
  it('is the tab itself in its left half and the next one in its right half', () => {
    expect(dropBefore(L, 'b', true)).toBe('b');
    expect(dropBefore(L, 'b', false)).toBe('c');
    expect(dropBefore(L, 'd', false)).toBeNull();
    expect(dropBefore(L, 'zz', true)).toBeNull();
  });
});
