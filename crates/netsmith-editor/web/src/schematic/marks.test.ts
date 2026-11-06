import { describe, expect, it } from 'vitest';

import { type MarkInput, marksOf, moduleIsShown } from './marks';

const base: MarkInput = { part: null, module: null, moduleParts: [], shownModule: null, cursor: [] };
const brain = ['C1', 'C2', 'U2'];

describe('what a selection marks in the drawing', () => {
  it('a picked part marks itself, wherever it is', () => {
    expect(marksOf({ ...base, part: 'C1', shownModule: 'brain' })).toEqual({ picked: ['C1'], marked: ['C1'] });
    expect(marksOf({ ...base, part: 'C1' })).toEqual({ picked: ['C1'], marked: ['C1'] });
  });
  it('a picked module marks its parts in the overview of all modules', () => {
    expect(marksOf({ ...base, module: 'brain', moduleParts: brain, shownModule: null })).toEqual({ picked: brain, marked: brain });
  });
  it('a picked module marks its parts on the sheet of another module', () => {
    expect(marksOf({ ...base, module: 'brain', moduleParts: brain, shownModule: 'link' }).marked).toEqual(brain);
  });
  it('a picked module marks nothing on its own sheet: the sheet is the module, and every part in a halo says nothing', () => {
    expect(marksOf({ ...base, module: 'brain', moduleParts: brain, shownModule: 'brain' })).toEqual({ picked: [], marked: [] });
  });
  it('but a module inside the sheet shown still marks its parts', () => {
    expect(marksOf({ ...base, module: 'brain.regulator', moduleParts: ['U3'], shownModule: 'brain' }).marked).toEqual(['U3']);
  });
  it('a part picked inside the module is marked on its sheet, and only that part', () => {
    // Picking a part replaces the module as the selection: the module is no longer selected.
    expect(marksOf({ ...base, part: 'C2', module: null, shownModule: 'brain' })).toEqual({ picked: ['C2'], marked: ['C2'] });
  });
  it('the parts the source’s cursor declares are marked with whatever is selected, once', () => {
    expect(marksOf({ ...base, module: 'brain', moduleParts: brain, shownModule: 'brain', cursor: ['U2'] })).toEqual({ picked: [], marked: ['U2'] });
    expect(marksOf({ ...base, part: 'C1', cursor: ['C1', 'C2'] })).toEqual({ picked: ['C1'], marked: ['C1', 'C2'] });
  });
  it('nothing is marked when nothing is selected', () => {
    expect(marksOf(base)).toEqual({ picked: [], marked: [] });
  });
  it('knows when the module is the one shown', () => {
    expect(moduleIsShown('brain', 'brain')).toBe(true);
    expect(moduleIsShown('brain', 'link')).toBe(false);
    expect(moduleIsShown('brain', null)).toBe(false);
    expect(moduleIsShown(null, null)).toBe(false);
  });
});
