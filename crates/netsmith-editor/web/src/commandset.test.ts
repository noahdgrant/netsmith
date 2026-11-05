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
