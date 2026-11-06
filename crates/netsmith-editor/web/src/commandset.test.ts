import { beforeAll, describe, expect, it } from 'vitest';

import { allCommands, chordClashes, chordText, find, parseChord, sameChord } from './commands';
import { registerAppCommands } from './commandset';

// The Schematic's keys are commands like any other: in the palette, in the cheat sheet, once.
const KEYS: Record<string, string[]> = {
  'schematic.sheet.previous': ['[', 'PageUp'],
  'schematic.sheet.next': [']', 'PageDown'],
  'schematic.fit': ['F', '0'],
  'schematic.fitwidth': ['W'],
  'schematic.actual': ['Shift+1'],
  'schematic.zoomselection': ['Shift+2'],
  'schematic.zoomin': ['+', '='],
  'schematic.zoomout': ['-'],
  'schematic.pan.left': ['ArrowLeft'],
  'schematic.pan.right': ['ArrowRight'],
  'schematic.pan.up': ['ArrowUp'],
  'schematic.pan.down': ['ArrowDown'],
  'schematic.pin.next': ['Tab'],
  'schematic.pin.previous': ['Shift+Tab'],
  'schematic.source': ['Enter'],
};

describe('the Schematic group of the command registry', () => {
  beforeAll(() => registerAppCommands());

  it('has every key of the view, with its chords, in the group Schematic', () => {
    for (const [id, keys] of Object.entries(KEYS)) {
      const c = find(id);
      expect(c, id).toBeDefined();
      expect(c!.group).toBe('Schematic');
      expect([c!.keys, ...(c!.alsoKeys ?? [])]).toEqual(keys);
    }
    expect(allCommands().filter((c) => c.group === 'Schematic').map((c) => c.id).sort()).toEqual(Object.keys(KEYS).sort());
  });

  it('shares no chord with a command of another group', () => {
    const others = allCommands().filter((c) => c.group !== 'Schematic');
    for (const c of allCommands().filter((x) => x.group === 'Schematic')) {
      for (const k of [c.keys!, ...(c.alsoKeys ?? [])]) {
        for (const o of others) {
          const same = [o.keys, ...(o.alsoKeys ?? [])].some((ok) => ok && sameChord(parseChord(ok), parseChord(k)));
          expect(same, `${k} of ${c.id} is also ${o.id}`).toBe(false);
        }
      }
    }
    // Two of the group never share one either.
    expect(chordClashes(allCommands().filter((c) => c.group === 'Schematic'))).toEqual([]);
  });

  it('shows its chords as text', () => {
    expect(chordText(find('schematic.zoomin')!.keys!, false)).toBe('+');
    expect(chordText('Shift+2', false)).toBe('Shift+2');
    expect(chordText('Shift+Tab', false)).toBe('Shift+Tab');
  });
});

// The source's commands (round 4): every action has an id, a group and, where it has one, a chord of its own.
const SOURCE_KEYS: Record<string, string[]> = {
  'nav.back': ['Escape'],
  'nav.history.back': ['Alt+Left'],
  'nav.history.forward': ['Alt+Right'],
  'goto.symbol': ['Mod+Shift+O'],
  'source.card.show': ['Mod+Alt+I'],
  'source.breadcrumbs.focus': ['Mod+Alt+B'],
  'source.fold.all': ['Mod+Alt+['],
  'source.fold.unfold': ['Mod+Alt+]'],
  'source.zoom.in': ['Mod+=', 'Mod++'],
  'source.zoom.out': ['Mod+-'],
  'source.zoom.reset': ['Mod+0'],
  'source.replace': ['Mod+H'],
  'source.wrap.toggle': ['Alt+Z'],
};
const SOURCE_SWITCHES = ['source.hints.toggle', 'source.cards.toggle', 'source.outline.toggle', 'source.wrap.toggle'];

describe('the Source group of the command registry', () => {
  beforeAll(() => registerAppCommands());

  it('has each action with its chords', () => {
    for (const [id, keys] of Object.entries(SOURCE_KEYS)) {
      const c = find(id);
      expect(c, id).toBeDefined();
      expect([c!.keys, ...(c!.alsoKeys ?? [])]).toEqual(keys);
    }
  });

  it('works from the editor: a chord with Ctrl or Alt, or one that says so', () => {
    for (const id of Object.keys(SOURCE_KEYS)) {
      const c = find(id)!;
      expect(c.inFields ?? /^(Mod|Alt)/.test(c.keys!), `${id} would be dead inside the editor`).toBe(true);
    }
  });

  it('has the switches, named for what they do next, and in the palette', () => {
    for (const id of SOURCE_SWITCHES) expect(find(id), id).toBeDefined();
    expect(find('source.hints.toggle')!.title).toMatch(/^Source: (Show|Hide) inline hints$/);
    expect(find('source.cards.toggle')!.title).toMatch(/^Source: (Show|Hide) hover cards$/);
    expect(find('source.outline.toggle')!.title).toMatch(/^Source: (Show|Hide) outline$/);
  });

  it('has go to symbol in the file and in the project, and the old chord goes to the one that suits the focus', () => {
    expect(find('goto.symbol.file')!.keys).toBeUndefined();
    expect(find('goto.symbol.project')!.keys).toBeUndefined();
    expect(find('goto.symbol')!.title).toBe('Go to symbol…');
  });

  it('shares no chord with another command (Escape is shared on purpose: it goes back, and clears the selection, when each applies)', () => {
    expect(chordClashes().filter((c) => !c.startsWith('Escape:'))).toEqual([]);
  });
});
