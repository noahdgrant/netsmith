import { describe, expect, it } from 'vitest';

import { KICAD_NOT_FOUND, kicadLine, kicadLinesFor } from './kicad';

describe('the line about KiCad in the findings list', () => {
  it('says a check that ran', () => {
    expect(kicadLine('ran', true)).toBe('with KiCad');
    expect(kicadLine('ran', null)).toBe('with KiCad');
  });
  it('says plainly that KiCad is not found, and what to do', () => {
    expect(KICAD_NOT_FOUND).toBe('KiCad not found: set KICAD_CLI or install KiCad');
    expect(kicadLine('not_run', false)).toBe(KICAD_NOT_FOUND);
    expect(kicadLine('unavailable', false)).toBe(KICAD_NOT_FOUND);
  });
  it('tells a tool that is there but was not asked from one that failed', () => {
    expect(kicadLine('not_run', true)).toBe('KiCad not run');
    expect(kicadLine('not_run', null)).toBe('KiCad not run');
    expect(kicadLine('unavailable', true)).toBe('KiCad unavailable');
  });
  it('lists every line a state can have', () => {
    expect(kicadLinesFor('ran')).toEqual(['with KiCad']);
    expect(kicadLinesFor('not_run').sort()).toEqual([KICAD_NOT_FOUND, 'KiCad not run'].sort());
    expect(kicadLinesFor('unavailable').sort()).toEqual([KICAD_NOT_FOUND, 'KiCad unavailable'].sort());
  });
});
