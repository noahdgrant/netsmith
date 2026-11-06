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

import { emptyProject } from './entry';

describe('emptyProject', () => {
  const err = (message: string) => ({ severity: 'error', message });
  it('is the file the checker names when the only error is that it has no board', () => {
    expect(emptyProject([err('src/main.nsm has no `board` to build')])).toEqual({ file: 'src/main.nsm' });
    expect(emptyProject([err('src/main.nsm has no `board` to build'), { severity: 'note', message: 'x' }])).toEqual({ file: 'src/main.nsm' });
  });
  it('is not one when there is another error, or none', () => {
    expect(emptyProject([err('src/main.nsm has no `board` to build'), err('unknown name `x`')])).toBeNull();
    expect(emptyProject([])).toBeNull();
    expect(emptyProject([{ severity: 'warning', message: 'a has no `board`' }])).toBeNull();
  });
});
