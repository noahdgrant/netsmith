import { describe, expect, it } from 'vitest';

import { HELP_REM, hasRoomForHelp, helpFolded, setHelpFolded } from './ViewHelp';

describe('the room for the help column', () => {
  const page = 1280; // --page-w, 80 rem at 16 px
  it('needs the page, and the column and its air on each side so the page stays centred', () => {
    const need = page + 2 * (HELP_REM + 1.5) * 16;
    expect(hasRoomForHelp(need, page, 16)).toBe(true);
    expect(hasRoomForHelp(need - 1, page, 16)).toBe(false);
  });
  it('has room at 1920 px and none at 1280 px or in a pane of a split', () => {
    expect(hasRoomForHelp(1920, page, 16)).toBe(true);
    expect(hasRoomForHelp(1280, page, 16)).toBe(false);
    expect(hasRoomForHelp(955, page, 16)).toBe(false);
  });
  it('has less at a larger size: the rem is bigger, so the column and the page both are', () => {
    // at 125 % the page is 1600 px and a rem is 20
    expect(hasRoomForHelp(1920, 1600, 20)).toBe(false);
    expect(hasRoomForHelp(2400, 1600, 20)).toBe(true);
  });
});

describe('what is folded', () => {
  it('is nothing at first, and each view is kept apart', () => {
    expect(helpFolded.value.bom).toBeUndefined();
    setHelpFolded('bom', true);
    expect(helpFolded.value.bom).toBe(true);
    expect(helpFolded.value.compare).toBeUndefined();
    setHelpFolded('bom', false);
    expect(helpFolded.value.bom).toBe(false);
  });
});
