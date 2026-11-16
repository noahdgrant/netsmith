import { describe, expect, it } from 'vitest';

import { appliesTo, faceWord, nextSide } from './side';

describe('the side of the board that is looked at', () => {
  it('goes front, back, both and round', () => {
    expect(nextSide('front')).toBe('back');
    expect(nextSide('back')).toBe('both');
    expect(nextSide('both')).toBe('front');
  });

  it('calls the file’s bottom the back and its top the front', () => {
    expect(faceWord('bottom')).toBe('back');
    expect(faceWord('top')).toBe('front');
  });

  it('draws the zones of the face it looks at, and all of them for both', () => {
    expect(appliesTo(['top'], 'front')).toBe(true);
    expect(appliesTo(['top'], 'back')).toBe(false);
    expect(appliesTo(['bottom'], 'back')).toBe(true);
    expect(appliesTo(['top', 'inner1', 'bottom'], 'back')).toBe(true);
    expect(appliesTo(['inner1'], 'front')).toBe(false);
    expect(appliesTo(['inner1'], 'both')).toBe(true);
  });
});
