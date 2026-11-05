import { beforeEach, describe, expect, it } from 'vitest';

import {
  allCommands,
  available,
  byRecent,
  chordClashes,
  chordLabels,
  chordOfEvent,
  chordText,
  clearRegistry,
  commandsForEvent,
  parseChord,
  provide,
  register,
  runCommand,
  sameChord,
  touchRecent,
  withKey,
} from './commands';

const ev = (key: string, mods: Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey', boolean>> = {}, code = '') => ({
  key,
  code,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe('chords', () => {
  it('parse and compare', () => {
    expect(parseChord('Mod+Shift+P')).toEqual({ mod: true, alt: false, shift: true, key: 'P' });
    expect(parseChord('Alt+1')).toEqual({ mod: false, alt: true, shift: false, key: '1' });
    expect(parseChord('Mod+\\')).toEqual({ mod: true, alt: false, shift: false, key: '\\' });
    expect(parseChord('?')).toEqual({ mod: false, alt: false, shift: false, key: '?' });
    expect(parseChord('Alt+Left').key).toBe('ArrowLeft');
    expect(parseChord('Esc').key).toBe('Escape');
    expect(parseChord('Mod++').key).toBe('+');
  });
  it('are read from an event', () => {
    expect(sameChord(chordOfEvent(ev('p', { ctrlKey: true, shiftKey: true })), parseChord('Mod+Shift+P'))).toBe(true);
    expect(sameChord(chordOfEvent(ev('P', { metaKey: true, shiftKey: true })), parseChord('Mod+Shift+P'))).toBe(true);
    expect(sameChord(chordOfEvent(ev('p', { ctrlKey: true })), parseChord('Mod+P'))).toBe(true);
    // `?` is Shift+/ but the shift is in the character.
    expect(sameChord(chordOfEvent(ev('?', { shiftKey: true })), parseChord('?'))).toBe(true);
    expect(sameChord(chordOfEvent(ev('3')), parseChord('3'))).toBe(true);
    expect(sameChord(chordOfEvent(ev('3')), parseChord('Alt+3'))).toBe(false);
  });
  it('read Alt+digit by the physical key, where Alt changes the character', () => {
    expect(sameChord(chordOfEvent(ev('¡', { altKey: true }, 'Digit1')), parseChord('Alt+1'))).toBe(true);
  });
  it('show as text, with symbols on a Mac', () => {
    expect(chordText('Mod+Shift+P', false)).toBe('Ctrl+Shift+P');
    expect(chordText('Mod+Shift+P', true)).toBe('⇧⌘P');
    expect(chordLabels('Alt+Left', false)).toEqual(['Alt', '←']);
    expect(chordLabels('Esc', false)).toEqual(['Esc']);
  });
});

describe('the registry', () => {
  let ran: string[];
  beforeEach(() => {
    clearRegistry();
    ran = [];
    register(
      { id: 'a', title: 'Alpha', group: 'G', keys: 'Mod+K', run: () => void ran.push('a') },
      { id: 'b', title: 'Beta', group: 'G', keys: 'Alt+1', alsoKeys: ['Mod+B'], when: () => false, run: () => void ran.push('b') },
    );
  });

  it('lists what can be done now, and everything for the cheat sheet', () => {
    expect(available().map((c) => c.id)).toEqual(['a']);
    expect(allCommands().map((c) => c.id)).toEqual(['a', 'b']);
  });
  it('runs a command that can be done, and refuses one that cannot', () => {
    expect(runCommand('a')).toBe(true);
    expect(runCommand('b')).toBe(false);
    expect(runCommand('missing')).toBe(false);
    expect(ran).toEqual(['a']);
  });
  it('replaces a command with the same id', () => {
    register({ id: 'a', title: 'Alpha 2', group: 'G', run: () => undefined });
    expect(allCommands().find((c) => c.id === 'a')!.title).toBe('Alpha 2');
    expect(allCommands()).toHaveLength(2);
  });
  it('takes commands from a provider', () => {
    provide(() => [{ id: 'board.X', title: 'Switch to X', group: 'Board', run: () => undefined }]);
    expect(available().map((c) => c.id)).toEqual(['a', 'board.X']);
  });
  it('finds the commands for a key event, only those that can be done', () => {
    expect(commandsForEvent(ev('k', { ctrlKey: true })).map((c) => c.id)).toEqual(['a']);
    expect(commandsForEvent(ev('1', { altKey: true })).map((c) => c.id)).toEqual([]);
    expect(commandsForEvent(ev('x'))).toEqual([]);
  });
  it('writes a tooltip with the key', () => {
    expect(withKey('Alpha', 'a')).toMatch(/^Alpha \(.+K\)$/);
    expect(withKey('Nothing', 'missing')).toBe('Nothing');
  });
  it('finds two commands on one chord', () => {
    expect(chordClashes()).toEqual([]);
    register({ id: 'c', title: 'Gamma', group: 'G', keys: 'Mod+B', run: () => undefined });
    expect(chordClashes()).toEqual(['Mod+B: b and c']);
  });
  it('treats a command whose test throws as unavailable', () => {
    register({ id: 'd', title: 'Delta', group: 'G', when: () => { throw new Error('no board'); }, run: () => undefined });
    expect(available().map((c) => c.id)).toEqual(['a']);
  });
});

describe('recently used', () => {
  it('moves the latest to the front, once, and keeps a bounded list', () => {
    expect(touchRecent(['a', 'b', 'c'], 'c')).toEqual(['c', 'a', 'b']);
    expect(touchRecent([], 'a')).toEqual(['a']);
    const many = Array.from({ length: 30 }, (_, i) => `x${i}`);
    expect(touchRecent(many, 'new')).toHaveLength(12);
  });
  it('puts used commands first and leaves the others in order', () => {
    const list = ['a', 'b', 'c', 'd'].map((id) => ({ id }));
    expect(byRecent(list, ['c', 'a']).map((x) => x.id)).toEqual(['c', 'a', 'b', 'd']);
    expect(byRecent(list, []).map((x) => x.id)).toEqual(['a', 'b', 'c', 'd']);
    expect(byRecent(list, ['gone']).map((x) => x.id)).toEqual(['a', 'b', 'c', 'd']);
  });
});
