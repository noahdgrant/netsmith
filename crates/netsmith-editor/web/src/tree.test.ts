import { describe, expect, it } from 'vitest';
import type { TreeFile } from './api';
import { groupByFolder, matches } from './tree';

const file = (path: string, ...kinds: string[]): TreeFile => ({
  path,
  role: 'module',
  readonly: false,
  defs: kinds.map((kind, i) => ({ kind, name: `${kind}${i}`, line: 1 })),
});

describe('groupByFolder', () => {
  const files = [
    file('src/sheets/mcu.nsm', 'module'),
    file('src/parts/bmi.nsm', 'part'),
    file('src/main.nsm'),
    file('src/parts/adc.nsm', 'part', 'part'),
    file('src/sheets/adc.nsm', 'module'),
  ];

  it('puts the files beside the root first, then folders by name', () => {
    const groups = groupByFolder(files);
    expect(groups.map((g) => g.name)).toEqual(['', 'parts', 'sheets']);
    expect(groups[1].files.map((f) => f.path)).toEqual(['src/parts/adc.nsm', 'src/parts/bmi.nsm']);
  });

  it('marks a folder of only parts as a library', () => {
    const library = Object.fromEntries(groupByFolder(files).map((g) => [g.name, g.library]));
    expect(library).toEqual({ '': false, parts: true, sheets: false });
  });

  it('does not take a folder with no definitions for a library', () => {
    expect(groupByFolder([file('src/x/empty.nsm')])[0].library).toBe(false);
  });
});

describe('matches', () => {
  it('finds a file by its path or by a definition in it, ignoring case', () => {
    const f = file('src/sheets/ADC.nsm', 'module');
    expect(matches(f, 'adc')).toBe(true);
    expect(matches(f, 'MODULE0')).toBe(true);
    expect(matches(f, 'gnss')).toBe(false);
    expect(matches(f, '  ')).toBe(true);
  });
});
