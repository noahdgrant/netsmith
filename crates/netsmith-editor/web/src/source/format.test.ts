import { describe, expect, it } from 'vitest';

import { textEdit } from './format';

const apply = (text: string, e: { from: number; to: number; insert: string }) => text.slice(0, e.from) + e.insert + text.slice(e.to);

describe('textEdit', () => {
  it('is null for the same text', () => {
    expect(textEdit('board B {}\n', 'board B {}\n')).toBeNull();
  });

  it('changes only what differs', () => {
    const before = 'board B {\n  .r: R\n    .c:C\n}\n';
    const after = 'board B {\n    .r: R\n    .c: C\n}\n';
    const e = textEdit(before, after)!;
    expect(apply(before, e)).toBe(after);
    expect(e.from).toBe('board B {\n  '.length);
    expect(e.to).toBe(before.length - 'C\n}\n'.length);
  });

  it('turns any text into any other', () => {
    for (const [a, b] of [['', 'x'], ['abc', ''], ['aaa', 'aa'], ['ab', 'aab'], ['xyz', 'xYz']]) {
      expect(apply(a, textEdit(a, b)!)).toBe(b);
    }
  });
});
