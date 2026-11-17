import { describe, expect, it } from 'vitest';

// macOS and Windows keep file names without regard to case: two files of the page that differ only
// by case (`Summary.tsx` and `summary.ts`) cannot both be checked out there, and the release build of
// the page fails on those systems. The glob lists every source of the page by its path.
const sources = Object.keys(import.meta.glob('./**/*.{ts,tsx,css}'));

describe('the page sources', () => {
  it('have no two file names that differ only by case', () => {
    const seen = new Map<string, string>();
    const clashes: string[] = [];
    for (const file of sources) {
      // A module is named without its extension (`./Summary` is Summary.ts or Summary.tsx), a style sheet with it.
      const stem = file.toLowerCase().replace(/\.tsx?$/, '');
      const other = seen.get(stem);
      if (other !== undefined) clashes.push(`${other} and ${file}`);
      else seen.set(stem, file);
    }
    expect(sources.length).toBeGreaterThan(50);
    expect(clashes).toEqual([]);
  });
});
