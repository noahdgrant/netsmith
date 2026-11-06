import { describe, expect, it } from 'vitest';

import { litNotes, noteAnchors, noteLabel, noteLocation, parseNoteAt, ringRect } from './notes';

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
  const notes = [{ anchors: ['R1'] }, { anchors: [] }, { anchors: ['C3', 'U2'] }, { anchors: ['R1', 'D1', 'R2'] }];
  it('are those any of whose parts is lit, not only the first', () => {
    expect(litNotes(notes, new Set(['R1']))).toEqual([0, 3]);
    expect(litNotes(notes, new Set(['C3', 'U9']))).toEqual([2]);
    expect(litNotes(notes, new Set(['R2']))).toEqual([3]);
    expect(litNotes(notes, new Set(['U2']))).toEqual([2]);
  });
  it('are none when nothing is lit, and a note of the board or a module (no part) is never lit by a part', () => {
    expect(litNotes(notes, new Set())).toEqual([]);
    expect(litNotes([{ anchors: [] }], new Set(['']))).toEqual([]);
  });
});

describe('the parts a note is about', () => {
  const byPath = new Map([
    ['led', 'D1'],
    ['supply', 'J1'],
    ['r1', 'R1'],
    ['stage.led', 'D2'],
    ['stage.r1', 'R2'],
  ]);
  const rel = '.led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance';
  it('are every part a relation names, in the order of its words, the server\'s first part first', () => {
    expect(noteAnchors({ first: 'D1', owner: '', relation: true, text: rel }, byPath)).toEqual(['D1', 'J1', 'R1']);
    expect(noteAnchors({ first: null, owner: '', relation: true, text: rel }, byPath)).toEqual(['D1', 'J1', 'R1']);
  });
  it('are read from the instance the note belongs to', () => {
    expect(noteAnchors({ first: 'D2', owner: 'stage', relation: true, text: '.led.current == .r1.current' }, byPath)).toEqual(['D2', 'R2']);
    expect(noteAnchors({ first: null, owner: 'stage', relation: true, text: '.supply.voltage == 3' }, byPath)).toEqual([]);
  });
  it('take the longest prefix that is a part, and ignore a name that is not one', () => {
    expect(noteAnchors({ first: null, owner: '', relation: true, text: '.stage.led.current == .nothing.x' }, byPath)).toEqual(['D2']);
  });
  it('are the first part only for a doc comment, whatever its words say', () => {
    expect(noteAnchors({ first: 'R1', owner: 'r1', relation: false, text: 'Sets .led.current for the whole board.' }, byPath)).toEqual(['R1']);
  });
  it('are none for the sheet\'s own note', () => {
    expect(noteAnchors({ first: null, owner: '', relation: false, text: 'A board.' }, byPath)).toEqual([]);
  });
});

describe('the words a screen reader says', () => {
  it('are Note: and the first 60 characters, cut with an ellipsis', () => {
    expect(noteLabel('Short.')).toBe('Note: Short.');
    const long = 'The quick brown fox jumps over the lazy dog and keeps running far away from here';
    const label = noteLabel(long);
    expect(label.startsWith('Note: The quick brown fox')).toBe(true);
    expect(label.endsWith('…')).toBe(true);
    expect(label.length).toBeLessThanOrEqual('Note: '.length + 61);
    expect(noteLabel('a\n  b')).toBe('Note: a b');
  });
});

describe("a note's focus ring", () => {
  const frame = { x: 10, y: 20, width: 60, height: 30, rx: 1 };
  it('stands 2 px clear of the frame on the screen: grown by 3 px (the offset and half the 2 px line), whatever the zoom', () => {
    for (const scale of [0.5, 1, 4]) {
      const r = ringRect(frame, scale);
      const grow = 3 / scale;
      expect(r.x).toBeCloseTo(frame.x - grow);
      expect(r.y).toBeCloseTo(frame.y - grow);
      expect(r.width).toBeCloseTo(frame.width + 2 * grow);
      expect(r.height).toBeCloseTo(frame.height + 2 * grow);
      // The ring's centre line is 3 px from the frame's edge on the screen, so the line's inner edge is 2 px away.
      expect((frame.x - r.x) * scale).toBeCloseTo(3);
    }
  });
  it("is clear of the frame's own stroke too: half of it (in the drawing's units) is added", () => {
    const thin = ringRect(frame, 4);
    const thick = ringRect({ ...frame, stroke: 0.8 }, 4);
    expect(frame.x - thick.x - (frame.x - thin.x)).toBeCloseTo(0.4);
    expect(thick.width - thin.width).toBeCloseTo(0.8);
  });
  it('rounds its corners a little more than the frame does', () => {
    expect(ringRect(frame, 2).rx).toBeCloseTo(1 + 1.5);
  });
  it('falls back to one unit a pixel when the zoom is not known', () => {
    expect(ringRect(frame, 0).x).toBeCloseTo(7);
    expect(ringRect(frame, Number.NaN).x).toBeCloseTo(7);
  });
});
