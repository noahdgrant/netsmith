import { describe, expect, it } from 'vitest';

import { titleFor } from './title';

describe('titleFor', () => {
  it('names the board', () => expect(titleFor({ board: 'TwoBoards/led', project: 'Greenhouse', dirty: false })).toBe('netsmith - TwoBoards/led'));
  it('falls back to the project, then to the name alone', () => {
    expect(titleFor({ board: null, project: 'Greenhouse', dirty: false })).toBe('netsmith - Greenhouse');
    expect(titleFor({ board: '', project: null, dirty: false })).toBe('netsmith');
  });
  it('puts a dot in front while there are unsaved changes', () => {
    expect(titleFor({ board: 'A/b', project: null, dirty: true })).toBe('● netsmith - A/b');
    expect(titleFor({ board: null, project: null, dirty: true })).toBe('● netsmith');
  });
});
