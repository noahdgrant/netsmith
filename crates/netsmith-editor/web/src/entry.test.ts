import { describe, expect, it } from 'vitest';

import { entryFileOf } from './entry';

const f = (...paths: string[]) => paths.map((path) => ({ path }));

describe('entryFileOf', () => {
  it('is src/main.nsm when it is there', () => expect(entryFileOf(f('src/power.nsm', 'src/main.nsm', 'std/parts/x.nsm'))).toBe('src/main.nsm'));
  it('is the first source file directly in src/ otherwise, never a module in a folder or a physical file', () => {
    expect(entryFileOf(f('src/sheets/a.nsm', 'src/main.physical.nsm', 'src/led.nsm'))).toBe('src/led.nsm');
  });
  it('is none for a project with no source file', () => expect(entryFileOf(f('netsmith.toml', 'std/a.nsm'))).toBeNull());
});
