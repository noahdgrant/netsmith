import { Text } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { nameColumn, revealTarget } from './reveal';

const doc = Text.of([
  '//! A board with a b in it.', // 1
  '', // 2
  'board Beta {', // 3
  '    .bus: Bus', // 4
  '}', // 5
  '',
  'part Led {', // 7
  '    .designator &= "D"', // 8
  '}',
]);

describe('revealTarget', () => {
  it('puts a cursor, not a one-letter selection, on the name of a definition the tree names by its line', () => {
    const t = revealTarget(doc, { line: 3, column: 1, endLine: 3, endColumn: 1 }, 'Beta');
    expect(t.anchor).toBe(t.head);
    expect(doc.sliceString(t.anchor, t.anchor + 4)).toBe('Beta');
    expect(t.line).toBe(3);
  });

  it('skips the keyword when the name is also in it', () => {
    const d = Text.of(['part part {', '}']);
    const t = revealTarget(d, { line: 1, column: 1, endLine: 1, endColumn: 1 }, 'part');
    expect(t.anchor).toBe(5);
  });

  it('does not take the name from inside a longer word', () => {
    expect(nameColumn('module Beta_two { .x: Beta }', 'Beta')).toBe(22);
    expect(nameColumn('board Alpha {', 'Beta')).toBeNull();
  });

  it('falls back to the first character of the line when the name is not on it', () => {
    const t = revealTarget(doc, { line: 8, column: 1, endLine: 8, endColumn: 1 }, 'Missing');
    expect(t.anchor).toBe(t.head);
    expect(t.anchor).toBe(doc.line(8).from + 4);
  });

  it('selects a span of a part that is a few characters long on one line', () => {
    const line = doc.line(7);
    const t = revealTarget(doc, { line: 7, column: 6, endLine: 7, endColumn: 9 });
    expect([t.anchor, t.head]).toEqual([line.from + 5, line.from + 8]);
    expect(doc.sliceString(t.anchor, t.head)).toBe('Led');
  });

  it('never selects one letter or nothing: a span of 0 or 1 is a cursor at its start', () => {
    for (const endColumn of [1, 2]) {
      const t = revealTarget(doc, { line: 3, column: 1, endLine: 3, endColumn });
      expect(t.anchor).toBe(t.head);
    }
  });

  it('puts a cursor at the start of a declaration that spans lines', () => {
    const t = revealTarget(doc, { line: 3, column: 1, endLine: 5, endColumn: 2 });
    expect(t.anchor).toBe(t.head);
    expect(t.anchor).toBe(doc.line(3).from);
  });

  it('keeps a place past the end of the file inside it', () => {
    const t = revealTarget(doc, { line: 99, column: 1, endLine: 99, endColumn: 1 });
    expect(t.line).toBe(doc.lines);
    expect(t.head).toBeLessThanOrEqual(doc.length);
  });
});
