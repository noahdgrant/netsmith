import { describe, expect, it } from 'vitest';

import { KICAD_NOT_FOUND, kicadLine } from './kicad';

describe('the line about KiCad in the findings list', () => {
  it('says a check that ran', () => {
    expect(kicadLine('ran')).toBe('with KiCad');
  });
  it('tells a tool that was not asked from one that failed', () => {
    expect(kicadLine('not_run')).toBe('KiCad not run');
    expect(kicadLine('unavailable')).toBe('KiCad unavailable');
  });
  it('says a tool that is not found once, as the title of its own state, with no line beside the button', () => {
    expect(KICAD_NOT_FOUND).toBe('KiCad not found');
  });
});
