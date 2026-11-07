import { describe, expect, it } from 'vitest';

import { COMMENT_FONTS, commentFontOf, nextCommentFont } from './prefs';

describe('the comment font', () => {
  it('defaults to upright, so the old slanted style is a choice and no longer the default', () => {
    expect(COMMENT_FONTS[0].id).toBe('upright');
    expect(commentFontOf('upright').style).toBe('normal');
    expect(commentFontOf('italic').style).toBe('italic');
  });

  it('cycles through every choice and back', () => {
    const seen = [COMMENT_FONTS[0].id];
    for (let i = 1; i < COMMENT_FONTS.length; i++) seen.push(nextCommentFont(seen[i - 1]));
    expect(new Set(seen).size).toBe(COMMENT_FONTS.length);
    expect(nextCommentFont(seen[seen.length - 1])).toBe(COMMENT_FONTS[0].id);
  });

  it('has a different look for each choice', () => {
    expect(new Set(COMMENT_FONTS.map((f) => `${f.style}/${f.family}`)).size).toBe(COMMENT_FONTS.length);
  });
});
