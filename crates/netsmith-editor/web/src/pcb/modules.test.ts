import { describe, expect, it } from 'vitest';

import { MODULE_SLOTS, groupsOf, inOrder, modulePaths, modulesAround, moduleToken, nextSlot, tintsOf, toggleModule } from './modules';

const parts = ['reg.ic', 'reg.ic.boost', 'reg.divider.rtop', 'reg.divider.rbottom', 'io.j1', 'bank[0]', 'bank[1]', 'led'];

describe('the modules lit on a board', () => {
  it('reads the modules from the paths, as the server does', () => {
    expect(modulePaths(parts)).toEqual(['bank', 'io', 'reg', 'reg.divider', 'reg.ic']);
    expect(modulesAround('reg.divider.rtop')).toEqual(['reg.divider', 'reg']);
    expect(modulesAround('led')).toEqual([]);
  });

  it('a click lights one, ctrl or shift adds one, and a click on one lit takes it off', () => {
    const one = toggleModule([], 'reg', false);
    expect(one).toEqual([{ path: 'reg', slot: 0 }]);
    const two = toggleModule(one, 'io', true);
    expect(two).toEqual([{ path: 'reg', slot: 0 }, { path: 'io', slot: 1 }]);
    // A click with no key on one not lit makes it the only one.
    expect(toggleModule(two, 'bank', false)).toEqual([{ path: 'bank', slot: 0 }]);
    // Off again, with or without the key; the others keep their colours, and the next takes the colour freed.
    const less = toggleModule(two, 'reg', false);
    expect(less).toEqual([{ path: 'io', slot: 1 }]);
    expect(nextSlot(less)).toBe(0);
    expect(toggleModule(less, 'bank', true)).toEqual([{ path: 'io', slot: 1 }, { path: 'bank', slot: 0 }]);
  });

  it('gives each module named its own colour in order, round again after the last', () => {
    const many = inOrder(['a', 'b', 'a', 'c', 'd', 'e', 'f', 'g']);
    expect(many.map((m) => m.slot)).toEqual([0, 1, 2, 3, 4, 5, 0]);
    expect(moduleToken(0)).toBe('--module-1');
    expect(moduleToken(MODULE_SLOTS)).toBe('--module-1');
  });

  it('tints each part by the innermost module lit, and a hovered module over them', () => {
    const lit = inOrder(['reg', 'reg.divider']);
    const t = tintsOf(parts, lit, null);
    expect(t.get('reg.ic')).toBe(0);
    expect(t.get('reg.divider.rtop')).toBe(1);
    expect(t.has('io.j1')).toBe(false);
    // A module hovered and not lit takes the colour it would take; one lit keeps its own.
    expect(tintsOf(parts, lit, 'io').get('io.j1')).toBe(2);
    expect(tintsOf(parts, lit, 'reg').get('reg.divider.rtop')).toBe(0);
    const groups = groupsOf(parts, lit, 'bank');
    expect(groups.map((g) => [g.path, g.slot, g.parts.length, g.hovered])).toEqual([
      ['reg', 0, 4, false],
      ['reg.divider', 1, 2, false],
      ['bank', 2, 2, true],
    ]);
  });
});
