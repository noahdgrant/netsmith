// The pure parts of what the editor draws: the guides, the folds, the gutter's marks, the find panel's
// count and the preferences' limits.

import { EditorState } from '@codemirror/state';
import { SearchQuery } from '@codemirror/search';
import { describe, expect, it } from 'vitest';

import { bracePairs } from './cm-fold';
import { levelsOf } from './cm-guides';
import { countMatches, countText } from './cm-search';
import { worst } from './cm-sev';
import { FONT, clampFont } from './prefs';

describe('indent guides', () => {
  it('counts a level for each four spaces and for a tab', () => {
    expect(levelsOf('board A {')).toBe(0);
    expect(levelsOf('    .x: Y')).toBe(1);
    expect(levelsOf('        .z &= 1')).toBe(2);
    expect(levelsOf('\t\t.z')).toBe(2);
    expect(levelsOf('      .odd')).toBe(1);
    expect(levelsOf('')).toBe(0);
  });
});

describe('folds', () => {
  const lines = ['board A {', '    .p: T {', '        .x &= 1', '    }', '    .q: U', '}', 'module M { }', 'x { // }'];
  it('pairs the line of an opening brace with the line of its close, nested', () => {
    expect([...bracePairs(lines)]).toEqual([[2, 4], [1, 6]]);
  });
  it('leaves a block on one line, and a brace in a comment or a string', () => {
    const m = bracePairs(['a { }', 'b { // {', '    "}"', '}']);
    expect([...m]).toEqual([[2, 4]]);
  });
  it('ignores a close that was never opened', () => {
    expect([...bracePairs(['}', 'a {', '}'])]).toEqual([[2, 3]]);
  });
});

describe('the mark in the gutter', () => {
  it('is the worst problem of the line, the error before the warning, and says how many more', () => {
    expect(worst([])).toBeNull();
    expect(worst([{ severity: 'warning', message: 'w' }])).toEqual({ severity: 'warning', message: 'w' });
    expect(worst([{ severity: 'warning', message: 'w' }, { severity: 'error', message: 'e\nmore' }])).toEqual({ severity: 'error', message: 'e (and 1 more on this line)' });
  });
});

describe('the find panel count', () => {
  const state = (text: string, from = 0, to = 0) => EditorState.create({ doc: text, selection: { anchor: from, head: to } });
  it('counts the matches and says which one the selection is on', () => {
    const s = state('Resistor, resistor, Resistor', 0, 8);
    const q = new SearchQuery({ search: 'resistor' });
    expect(countMatches(s, q)).toEqual({ total: 3, current: 1, capped: false, invalid: false });
    expect(countMatches(s, new SearchQuery({ search: 'resistor', caseSensitive: true }))).toMatchObject({ total: 1, current: 0 });
    expect(countMatches(s, new SearchQuery({ search: 'resistor', wholeWord: true, caseSensitive: true }))).toMatchObject({ total: 1 });
  });
  it('is invalid for a regular expression that is not one, and empty for no query', () => {
    expect(countMatches(state('a'), new SearchQuery({ search: '(', regexp: true }))).toMatchObject({ invalid: true });
    expect(countMatches(state('a'), new SearchQuery({ search: '' }))).toMatchObject({ total: 0, invalid: false });
  });
  it('stops counting at a thousand', () => {
    const c = countMatches(state('a '.repeat(2000)), new SearchQuery({ search: 'a' }));
    expect(c.capped).toBe(true);
    expect(countText(c, true)).toBe('999+ found');
  });
  it('reads as a person would say it', () => {
    expect(countText({ total: 12, current: 3, capped: false, invalid: false }, true)).toBe('3 of 12');
    expect(countText({ total: 12, current: 0, capped: false, invalid: false }, true)).toBe('12 found');
    expect(countText({ total: 0, current: 0, capped: false, invalid: false }, true)).toBe('No results');
    expect(countText({ total: 0, current: 0, capped: false, invalid: true }, true)).toBe('Not a pattern');
    expect(countText({ total: 0, current: 0, capped: false, invalid: false }, false)).toBe('');
  });
});

describe('the size of the text', () => {
  it('stays between the smallest and the largest, in whole pixels', () => {
    expect(clampFont(3)).toBe(FONT.min);
    expect(clampFont(99)).toBe(FONT.max);
    expect(clampFont(13.4)).toBe(13);
    expect(FONT.initial).toBeGreaterThanOrEqual(FONT.min);
  });
});
