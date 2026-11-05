import { describe, expect, it } from 'vitest';

import {
  type Layout,
  MIN_PANE,
  clampSplit,
  closePane,
  companion,
  defaultLayout,
  dropAt,
  dropTab,
  openFrom,
  parseLayout,
  sectionOfKey,
  show,
  splitRight,
  splitWith,
  undoOpen,
  visiblePanes,
  whereShown,
} from './layout';

const two = (a: Layout['panes'][0], b: Layout['panes'][0], focus = 0): Layout => ({ panes: [a, b], split: 0.5, focus });

describe('parseLayout', () => {
  it('takes a good layout as it is', () => {
    expect(parseLayout({ panes: ['source', 'schematic'], split: 0.3, focus: 1 })).toEqual({
      panes: ['source', 'schematic'],
      split: 0.3,
      focus: 1,
    });
  });
  it('drops what it cannot trust', () => {
    expect(parseLayout(null)).toEqual(defaultLayout);
    expect(parseLayout({ panes: ['nonsense'] })).toEqual(defaultLayout);
    expect(parseLayout({ panes: ['bom', 'bom'] }).panes).toEqual(['bom']);
    expect(parseLayout({ panes: ['bom', 'pcb', 'source'] }).panes).toEqual(['bom', 'pcb']);
    expect(parseLayout({ panes: ['bom'], split: 7, focus: 1 })).toEqual({ panes: ['bom'], split: 0.9, focus: 0 });
  });
});

describe('show', () => {
  it('replaces the section of a pane', () => {
    expect(show(defaultLayout, 'bom').panes).toEqual(['bom']);
  });
  it('swaps when the other pane has the section, so no section is in both', () => {
    const l = show(two('source', 'schematic'), 'schematic', 0);
    expect(l.panes).toEqual(['schematic', 'source']);
    expect(l.focus).toBe(0);
  });
});

describe('splitting', () => {
  it('splits right: the old section stays on the left', () => {
    expect(splitWith(defaultLayout, 'source', 'right').panes).toEqual(['schematic', 'source']);
    expect(splitWith(defaultLayout, 'source', 'left').panes).toEqual(['source', 'schematic']);
  });
  it('gives a partner to the old pane when the dragged tab was its section', () => {
    expect(splitWith(defaultLayout, 'schematic', 'right').panes).toEqual(['source', 'schematic']);
    expect(companion('source')).toBe('schematic');
  });
  it('puts a tab dropped on one side of two panes into that pane', () => {
    expect(splitWith(two('source', 'schematic'), 'bom', 'left').panes).toEqual(['bom', 'schematic']);
  });
  it('splits right with a partner for what is shown', () => {
    expect(splitRight({ ...defaultLayout, panes: ['source'] }).panes).toEqual(['source', 'schematic']);
    expect(splitRight(defaultLayout).panes).toEqual(['schematic', 'source']);
  });
  it('closes a pane', () => {
    expect(closePane(two('source', 'schematic'), 1).panes).toEqual(['source']);
    expect(closePane(two('source', 'schematic'), 0).panes).toEqual(['schematic']);
    expect(closePane(defaultLayout, 0)).toBe(defaultLayout);
  });
});

describe('a narrow window', () => {
  const l = two('source', 'schematic', 1);
  it('shows the focused pane alone and keeps the layout', () => {
    expect(visiblePanes(l, 1366)).toEqual([{ section: 'schematic', index: 1 }]);
    expect(visiblePanes(l, 2000)).toHaveLength(2);
    expect(l.panes).toEqual(['source', 'schematic']);
  });
  it('finds where a section is on screen', () => {
    expect(whereShown(l, 1366, 'source')).toBeNull();
    expect(whereShown(l, 2000, 'source')).toBe(0);
  });
});

describe('clampSplit', () => {
  it('keeps both panes at least the minimum wide', () => {
    expect(clampSplit(0.01, 2000)).toBeCloseTo(MIN_PANE / 2000);
    expect(clampSplit(0.99, 2000)).toBeCloseTo(1 - MIN_PANE / 2000);
    expect(clampSplit(0.4, 2000)).toBe(0.4);
  });
  it('gives half when there is no room, or no number', () => {
    expect(clampSplit(0.2, 600)).toBe(0.5);
    expect(clampSplit(NaN, 2000)).toBe(0.5);
  });
});

describe('click-through', () => {
  it('replaces the section of the only pane and remembers it for Back', () => {
    const r = openFrom(defaultLayout, 1366, 'source', 0);
    expect(r.layout.panes).toEqual(['source']);
    expect(r.back).toEqual({ pane: 0, section: 'schematic' });
    expect(undoOpen(r.layout, r.back!).panes).toEqual(['schematic']);
  });
  it('opens in the other pane of a split', () => {
    const r = openFrom(two('schematic', 'bom'), 2000, 'source', 0);
    expect(r.layout.panes).toEqual(['schematic', 'source']);
    expect(r.back).toEqual({ pane: 1, section: 'bom' });
    expect(undoOpen(r.layout, r.back!).panes).toEqual(['schematic', 'bom']);
  });
  it('only moves the focus when the source is already on screen', () => {
    const r = openFrom(two('source', 'schematic', 1), 2000, 'source', 1);
    expect(r.layout.panes).toEqual(['source', 'schematic']);
    expect(r.layout.focus).toBe(0);
    expect(r.back).toBeNull();
  });
  it('in a narrow window with a saved split, uses the visible pane', () => {
    const r = openFrom(two('source', 'schematic', 1), 1366, 'source', 1);
    expect(r.layout.panes).toEqual(['schematic', 'source']);
    expect(r.layout.focus).toBe(1);
    expect(r.back).toEqual({ pane: 1, section: 'schematic' });
  });
});

describe('dropping a tab', () => {
  it('splits a one-pane window by the half it is dropped on', () => {
    expect(dropAt(defaultLayout, 2000, 1500, 0, 2000)).toEqual({ side: 'right' });
    expect(dropAt(defaultLayout, 2000, 300, 0, 2000)).toEqual({ side: 'left' });
    expect(dropTab(defaultLayout, 'source', { side: 'left' }).panes).toEqual(['source', 'schematic']);
  });
  it('moves a tab to the pane it is dropped on, swapping when it was in the other', () => {
    const l = two('source', 'schematic');
    expect(dropAt(l, 2000, 1500, 0, 2000)).toEqual({ pane: 1 });
    expect(dropTab(l, 'source', { pane: 1 }).panes).toEqual(['schematic', 'source']);
  });
  it('does not split a narrow window', () => {
    expect(dropAt(defaultLayout, 1366, 900, 0, 1366)).toBeNull();
  });
});

describe('sectionOfKey', () => {
  it('numbers the sections from 1', () => {
    expect(sectionOfKey('1')).toBe('source');
    expect(sectionOfKey('2')).toBe('schematic');
    expect(sectionOfKey('7')).toBe('build');
    expect(sectionOfKey('8')).toBeNull();
    expect(sectionOfKey('a')).toBeNull();
  });
});
