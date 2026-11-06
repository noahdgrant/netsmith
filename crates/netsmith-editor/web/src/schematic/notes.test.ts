import { describe, expect, it } from 'vitest';

import { litNotes, noteLocation, parseNoteAt } from './notes';

describe('where a note is written', () => {
  it('is the file and the line after the last colon', () => {
    expect(parseNoteAt('src/main.nsm:12')).toEqual({ file: 'src/main.nsm', line: 12 });
    expect(parseNoteAt('C:/work/x.nsm:3')).toEqual({ file: 'C:/work/x.nsm', line: 3 });
  });
  it('is nothing for a text that is not a place', () => {
    for (const bad of [null, undefined, '', 'src/main.nsm', 'src/main.nsm:', 'src/main.nsm:x', ':12', 'src/main.nsm:0', 'src/main.nsm:1.5', 'src/main.nsm:-4']) {
      expect(parseNoteAt(bad), String(bad)).toBeNull();
    }
  });
  it('opens at the start of the line, a cursor and no selection', () => {
    expect(noteLocation({ file: 'src/a.nsm', line: 7 })).toEqual({ file: 'src/a.nsm', line: 7, column: 1, endLine: 7, endColumn: 1 });
  });
});

describe('the notes that are lit with their parts', () => {
  const notes = [{ ref: 'R1' }, { ref: null }, { ref: 'C3' }, { ref: 'R1' }];
  it('are those whose first part is lit', () => {
    expect(litNotes(notes, new Set(['R1']))).toEqual([0, 3]);
    expect(litNotes(notes, new Set(['C3', 'U9']))).toEqual([2]);
  });
  it('are none when nothing is lit, and a note of the board or a module (no part) is never lit by a part', () => {
    expect(litNotes(notes, new Set())).toEqual([]);
    expect(litNotes([{ ref: null }], new Set(['']))).toEqual([]);
  });
});
