import { beforeEach, describe, expect, it } from 'vitest';

import type { BoardDetail, PartInfo } from './api';
import {
  cardFor,
  declarationOf,
  describe as describeSel,
  exists,
  modulesOf,
  moduleSel,
  netSel,
  partPathsOf,
  partSel,
  refdesOf,
  revealRequest,
  sameSelection,
  selectAndReveal,
  selection,
  selectionOfCard,
  setSelection,
  under,
} from './selection';

const at = (file: string, line: number) => ({ file, line, column: 1, endLine: line, endColumn: 10 });
const part = (refdes: string, path: string, scopes = 0): PartInfo => ({
  refdes,
  kind: 'Resistor',
  value: '1k',
  package: '0402',
  path,
  lcsc: null,
  assembled: true,
  decl: at('src/m.nsm', 5),
  scopes: Array.from({ length: scopes }, (_, i) => at('src/main.nsm', 10 + i)),
});

const detail: BoardDetail = {
  name: 'B',
  file: 'src/main.nsm',
  notes: [],
  parts: [part('R1', 'stage1.limit', 1), part('C1', 'stage1.cap', 1), part('U1', 'top.inner.reg', 2), part('J1', 'j1', 0)],
  nets: [
    { name: 'GND', named: true, pins: [{ refdes: 'R1', pin: '2' }, { refdes: 'C1', pin: '2' }, { refdes: 'J1', pin: '2' }] },
    { name: 'RAIL', named: true, pins: [{ refdes: 'R1', pin: '1' }] },
  ],
};

describe('the shared selection', () => {
  beforeEach(() => {
    selection.value = null;
    revealRequest.value = null;
  });

  it('is set once and read everywhere', () => {
    setSelection(partSel('stage1.limit'));
    expect(selection.value).toEqual({ kind: 'part', key: 'stage1.limit' });
    setSelection(null);
    expect(selection.value).toBeNull();
  });

  it('does not notify for the same selection', () => {
    setSelection(netSel('GND'));
    const first = selection.value;
    setSelection(netSel('GND'));
    expect(selection.value).toBe(first);
  });

  it('compares by kind and key', () => {
    expect(sameSelection(partSel('a'), partSel('a'))).toBe(true);
    expect(sameSelection(partSel('a'), netSel('a'))).toBe(false);
    expect(sameSelection(null, null)).toBe(true);
    expect(sameSelection(partSel('a'), null)).toBe(false);
  });

  it('asks the views to reveal, and again for the same thing', () => {
    selectAndReveal(partSel('j1'));
    const a = revealRequest.value!.nonce;
    selectAndReveal(partSel('j1'));
    expect(revealRequest.value!.nonce).toBeGreaterThan(a);
    expect(selection.value).toEqual(partSel('j1'));
  });
});

describe('what a selection stands for', () => {
  it('keys parts by instance path, never by designator', () => {
    expect(refdesOf(detail, partSel('stage1.limit'))).toEqual(new Set(['R1']));
    expect(refdesOf(detail, partSel('R1'))).toEqual(new Set());
  });
  it('gives the parts on a net', () => {
    expect(refdesOf(detail, netSel('RAIL'))).toEqual(new Set(['R1']));
    expect(refdesOf(detail, netSel('GND'))).toEqual(new Set(['R1', 'C1', 'J1']));
  });
  it('gives the parts under a module at any depth, and not a sibling with the same prefix', () => {
    expect(refdesOf(detail, moduleSel('stage1'))).toEqual(new Set(['R1', 'C1']));
    expect(refdesOf(detail, moduleSel('top'))).toEqual(new Set(['U1']));
    expect(partPathsOf(detail, moduleSel('top.inner'))).toEqual(['top.inner.reg']);
    expect(under('stage10.x', 'stage1')).toBe(false);
    expect(under('stage1.x', 'stage1')).toBe(true);
  });
  it('knows whether the board still has it', () => {
    expect(exists(detail, partSel('stage1.limit'))).toBe(true);
    expect(exists(detail, partSel('gone'))).toBe(false);
    expect(exists(detail, netSel('GND'))).toBe(true);
    expect(exists(detail, moduleSel('stage1'))).toBe(true);
    expect(exists(detail, moduleSel('j1'))).toBe(false);
    // Before the board is read nothing is dropped.
    expect(exists(null, partSel('x'))).toBe(true);
  });
  it('describes it for a person', () => {
    expect(describeSel(detail, partSel('stage1.limit')).title).toBe('R1 1k');
    expect(describeSel(detail, netSel('GND')).sub).toBe('net · 3 pins');
    expect(describeSel(detail, moduleSel('stage1')).sub).toBe('module · 2 parts');
  });
});

describe('modules', () => {
  it('are the instance paths above the parts, with the declaration of each', () => {
    const mods = modulesOf(detail);
    expect(mods.map((m) => m.path)).toEqual(['stage1', 'top', 'top.inner']);
    // `top.inner.reg` has [.inner of top, .top]; depth 1 is the last, depth 2 the first.
    expect(mods.find((m) => m.path === 'top')!.decl).toEqual(at('src/main.nsm', 11));
    expect(mods.find((m) => m.path === 'top.inner')!.decl).toEqual(at('src/main.nsm', 10));
  });
  it('give the declaration of a part or a module, and none for a net', () => {
    expect(declarationOf(detail, partSel('stage1.limit'))).toEqual(at('src/m.nsm', 5));
    expect(declarationOf(detail, moduleSel('stage1'))).toEqual(at('src/main.nsm', 10));
    expect(declarationOf(detail, netSel('GND'))).toBeNull();
  });
});

describe('cards', () => {
  const cards = [
    { id: 'RAIL', kind: 'net' as const, key: 'RAIL' },
    { id: 'stage1', kind: 'module' as const, key: 'stage1' },
    { id: 'top.inner', kind: 'module' as const, key: 'top.inner' },
    { id: 'top', kind: 'module' as const, key: 'top' },
    { id: 'j1', kind: 'part' as const, key: 'j1' },
  ];
  it('shows a net on its rail and a module on its card', () => {
    expect(cardFor(netSel('RAIL'), cards)).toBe('RAIL');
    expect(cardFor(netSel('GND'), cards)).toBeNull();
    expect(cardFor(moduleSel('stage1'), cards)).toBe('stage1');
    expect(cardFor(partSel('j1'), cards)).toBe('j1');
  });
  it('shows a part on the card of the nearest module above it', () => {
    expect(cardFor(partSel('stage1.limit'), cards)).toBe('stage1');
    expect(cardFor(partSel('top.inner.reg'), cards)).toBe('top.inner');
    expect(cardFor(partSel('top.other.x'), cards)).toBe('top');
    expect(cardFor(partSel('stage10.x'), cards)).toBeNull();
    expect(cardFor(null, cards)).toBeNull();
  });
  it('maps a card back to what it selects', () => {
    expect(selectionOfCard(cards, 'RAIL')).toEqual(netSel('RAIL'));
    expect(selectionOfCard(cards, 'top')).toEqual(moduleSel('top'));
    expect(selectionOfCard(cards, 'nope')).toBeNull();
  });
});
