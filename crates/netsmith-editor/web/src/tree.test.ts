import { describe, expect, it } from 'vitest';
import type { FileRole, TreeFile } from './api';
import {
  ancestorIds,
  buildTree,
  compactOverrides,
  countTitle,
  fileIcon,
  filesOf,
  filterTree,
  flatten,
  highlight,
  keyMove,
  matches,
  parseOverrides,
  setAll,
  type DefNode,
  type FolderNode,
  type Row,
  type SectionNode,
} from './tree';

const file = (path: string, kinds: string[] = [], role: FileRole = 'module'): TreeFile => ({
  path,
  role,
  readonly: false,
  defs: kinds.map((kind, i) => ({ kind, name: `${kind[0].toUpperCase()}${kind.slice(1)}${i}`, line: i + 1 })),
});

// An invented project: nested folders, a library, a chain of single folders.
const FILES: TreeFile[] = [
  file('src/greenhouse.nsm', ['board', 'system'], 'top_level'),
  file('src/main.nsm', ['module']),
  file('src/sheets/Zeta.nsm', ['module']),
  file('src/sheets/adc10.nsm', ['module']),
  file('src/sheets/adc2.nsm', ['module']),
  file('src/sheets/power/ldo.nsm', ['module']),
  file('src/proposed/alarm.nsm', ['module']),
  file('src/proposed/superseded/relay_a.nsm', ['module']),
  file('src/parts/sensors/bme.nsm', ['part']),
  file('src/parts/led.nsm', ['part', 'part']),
  file('src/legacy/vendor/archive/old.nsm', ['module']),
  file('src/main.physical.nsm', [], 'physical'),
  file('netsmith.toml', [], 'manifest'),
  file('std/bus.nsm', ['interface'], 'std'),
  file('std/mfg/jlcpcb.nsm', ['interface'], 'std'),
];

const tree = buildTree(FILES);
const section = (role: FileRole) => tree.find((s) => s.role === role) as SectionNode;
const names = (nodes: SectionNode['children']) => nodes.map((n) => (n.type === 'folder' ? `${n.segments.join('/')}/` : n.name));
const folder = (nodes: SectionNode['children'], first: string) => nodes.find((n) => n.type === 'folder' && n.segments[0] === first) as FolderNode;

describe('buildTree', () => {
  it('makes a section for each role that has files, in a fixed order', () => {
    expect(tree.map((s) => s.role)).toEqual(['top_level', 'module', 'physical', 'manifest', 'std']);
    expect(buildTree([FILES[1]]).map((s) => s.role)).toEqual(['module']);
  });

  it('puts a nested folder inside its parent, never beside it as `a/b`', () => {
    const proposed = folder(section('module').children, 'proposed');
    expect(names(proposed.children)).toEqual(['superseded/', 'alarm.nsm']);
    expect(names(section('module').children)).not.toContain('proposed/superseded/');
  });

  it('shows folders before files, each by name, ignoring case and counting numbers', () => {
    expect(names(section('module').children)).toEqual(['legacy/vendor/archive/', 'parts/', 'proposed/', 'sheets/', 'main.nsm']);
    const sheets = folder(section('module').children, 'sheets');
    expect(names(sheets.children)).toEqual(['power/', 'adc2.nsm', 'adc10.nsm', 'Zeta.nsm']);
  });

  it('is the same whatever order the files arrive in', () => {
    const shuffled = buildTree([...FILES].reverse());
    expect(JSON.stringify(shuffled)).toBe(JSON.stringify(tree));
  });

  it('shows a chain of single folders as one row with every folder of it', () => {
    const chain = folder(section('module').children, 'legacy');
    expect(chain.segments).toEqual(['legacy', 'vendor', 'archive']);
    expect(names(chain.children)).toEqual(['old.nsm']);
    expect(chain.id).toBe('d:module:legacy/vendor/archive');
  });

  it('does not join a folder that has files of its own or two folders', () => {
    expect(folder(section('module').children, 'proposed').segments).toEqual(['proposed']);
    expect(folder(section('module').children, 'sheets').segments).toEqual(['sheets']);
  });

  it('counts the files below a folder, at any depth, and marks a library', () => {
    const parts = folder(section('module').children, 'parts');
    expect(parts.files).toBe(2);
    expect(parts.library).toBe(true);
    expect(folder(section('module').children, 'sheets').files).toBe(4);
    expect(folder(section('module').children, 'sheets').library).toBe(false);
    expect(countTitle(parts)).toBe('2 files, all of them parts: a library');
    expect(countTitle(folder(section('module').children, 'legacy'))).toBe('1 file');
  });

  it('does not take a folder with no definitions for a library', () => {
    const t = buildTree([file('src/x/empty.nsm'), file('src/x/y.nsm')]);
    expect((t[0].children[0] as FolderNode).library).toBe(false);
  });

  it('shows the standard library below std/, and files outside src/ as they are', () => {
    expect(names(section('std').children)).toEqual(['mfg/', 'bus.nsm']);
    expect(names(buildTree([file('board.nsm', [], 'top_level')])[0].children)).toEqual(['board.nsm']);
  });

  it('puts the definitions of a file under it, in the order of the file', () => {
    const [greenhouse] = filesOf(section('top_level').children);
    expect(greenhouse.children.map((d) => `${d.def.kind}:${d.name}`)).toEqual(['board:Board0', 'system:System1']);
  });
});

describe('what is open', () => {
  it('starts with the sections open but the standard library, a library of parts closed and the definitions of a top-level file shown', () => {
    const rows = flatten(tree, {});
    const open = Object.fromEntries(rows.map((r) => [r.id, r.open]));
    expect(open['s:module']).toBe(true);
    expect(open['s:std']).toBe(false);
    expect(open['d:module:parts']).toBe(false);
    expect(open['f:src/greenhouse.nsm']).toBe(true);
    expect(open['f:src/main.nsm']).toBe(false); // a file with a definition: closed
    expect(rows.some((r) => r.id === 'f:src/parts/led.nsm')).toBe(false);
    expect(rows.some((r) => r.node.type === 'def' && r.node.name === 'Board0')).toBe(true);
  });

  it('gives each row its level and parent', () => {
    const rows = flatten(tree, {});
    const by = (id: string) => rows.find((r) => r.id === id) as Row;
    expect(by('s:module').level).toBe(1);
    expect(by('d:module:proposed').level).toBe(2);
    expect(by('d:module:proposed').parent).toBe('s:module');
    expect(by('d:module:proposed/superseded').level).toBe(3);
    expect(by('f:src/proposed/superseded/relay_a.nsm').level).toBe(4);
    expect(by('f:src/proposed/superseded/relay_a.nsm').parent).toBe('d:module:proposed/superseded');
  });

  it('follows what the person opened or closed over the defaults', () => {
    const rows = flatten(tree, { 'd:module:parts': true, 's:module': false });
    expect(rows.some((r) => r.id === 's:module')).toBe(true);
    expect(rows.some((r) => r.id === 'd:module:parts')).toBe(false);
    const again = flatten(tree, { 'd:module:parts': true });
    expect(again.some((r) => r.id === 'f:src/parts/led.nsm')).toBe(true);
  });

  it('opens or closes every folder and file, not the sections', () => {
    const all = setAll(tree, true, {});
    const rows = flatten(tree, all);
    expect(rows.some((r) => r.id === 'f:src/parts/sensors/bme.nsm')).toBe(true);
    expect(rows.find((r) => r.id === 's:std')?.open).toBe(false);
    const none = flatten(tree, setAll(tree, false, all));
    expect(none.filter((r) => r.node.type === 'folder').length).toBe(4);
    expect(none.some((r) => r.node.type === 'file')).toBe(true); // the files beside a closed folder: main.nsm
    expect(none.some((r) => r.id === 'f:src/sheets/adc2.nsm')).toBe(false);
  });

  it('names what has to be open to see a file', () => {
    expect(ancestorIds(tree, 'src/proposed/superseded/relay_a.nsm')).toEqual(['s:module', 'd:module:proposed', 'd:module:proposed/superseded']);
    expect(ancestorIds(tree, 'nothing.nsm')).toEqual([]);
  });

  it('keeps only what differs from the default and is still in the tree', () => {
    const kept = compactOverrides(tree, { 'd:module:parts': true, 'd:module:sheets': true, 'd:module:gone': false, 's:std': false });
    expect(kept).toEqual({ 'd:module:parts': true });
  });

  it('reads a saved state that is damaged as nothing', () => {
    expect(parseOverrides({ a: true, b: 'x', c: 1, d: false })).toEqual({ a: true, d: false });
    expect(parseOverrides(null)).toEqual({});
    expect(parseOverrides([true])).toEqual({});
    expect(parseOverrides('text')).toEqual({});
  });
});

describe('the filter', () => {
  it('finds a file by its path or by a definition in it, ignoring case', () => {
    const f = file('src/sheets/ADC.nsm', ['module']);
    expect(matches(f, 'adc')).toBe(true);
    expect(matches(f, 'MODULE0')).toBe(true);
    expect(matches(f, 'gnss')).toBe(false);
    expect(matches(f, '  ')).toBe(true);
  });

  it('keeps the folders above a match, open, and drops the rest', () => {
    const found = filterTree(tree, 'relay');
    expect(found.map((s) => s.role)).toEqual(['module']);
    const rows = flatten(found, {}, 'relay');
    expect(rows.map((r) => r.id)).toEqual([
      's:module',
      'd:module:proposed',
      'd:module:proposed/superseded',
      'f:src/proposed/superseded/relay_a.nsm',
    ]);
    expect(rows.every((r) => r.open !== false)).toBe(true);
  });

  it('opens even what is closed, a library and the standard library, when it has a match', () => {
    const rows = flatten(filterTree(tree, 'bme'), {}, 'bme');
    expect(rows.map((r) => r.id)).toEqual(['s:module', 'd:module:parts', 'd:module:parts/sensors', 'f:src/parts/sensors/bme.nsm']);
    expect(flatten(filterTree(tree, 'jlcpcb'), {}, 'jlcpcb').some((r) => r.id === 'f:std/mfg/jlcpcb.nsm')).toBe(true);
  });

  it('shows the definitions that match, and only those', () => {
    const rows = flatten(filterTree(tree, 'system1'), {}, 'system1');
    expect(rows.map((r) => r.node.type)).toEqual(['section', 'file', 'def']);
    expect((rows[2].node as DefNode).name).toBe('System1');
  });

  it('counts what it shows', () => {
    const found = filterTree(tree, 'adc');
    expect(found[0].files).toBe(2);
    expect((found[0].children[0] as FolderNode).files).toBe(2);
  });

  it('is the tree itself with no filter, and empty when nothing matches', () => {
    expect(filterTree(tree, '  ')).toBe(tree);
    expect(filterTree(tree, 'zzz')).toEqual([]);
  });

  it('marks the match in a name', () => {
    expect(highlight('relay_a.nsm', 'LAY')).toEqual([
      { text: 're', hit: false },
      { text: 'lay', hit: true },
      { text: '_a.nsm', hit: false },
    ]);
    expect(highlight('relay', '')).toEqual([{ text: 'relay', hit: false }]);
    expect(highlight('relay', 'x')).toEqual([{ text: 'relay', hit: false }]);
  });
});

describe('the keyboard', () => {
  const rows = flatten(tree, {});
  const id = (n: number) => rows[n].id;
  const index = (rid: string) => rows.findIndex((r) => r.id === rid);

  it('moves with Up and Down, and stops at the ends', () => {
    expect(keyMove(rows, id(0), 'ArrowDown')).toEqual({ focus: id(1) });
    expect(keyMove(rows, id(3), 'ArrowUp')).toEqual({ focus: id(2) });
    expect(keyMove(rows, id(0), 'ArrowUp')).toEqual({ focus: id(0) });
    expect(keyMove(rows, id(rows.length - 1), 'ArrowDown')).toEqual({ focus: id(rows.length - 1) });
  });

  it('goes to the first and the last row with Home and End', () => {
    expect(keyMove(rows, id(5), 'Home')).toEqual({ focus: id(0) });
    expect(keyMove(rows, id(0), 'End')).toEqual({ focus: id(rows.length - 1) });
  });

  it('opens a closed folder with Right, then goes into it', () => {
    expect(keyMove(rows, 'd:module:parts', 'ArrowRight')).toEqual({ toggle: 'd:module:parts', open: true });
    expect(keyMove(rows, 'd:module:proposed', 'ArrowRight')).toEqual({ focus: rows[index('d:module:proposed') + 1].id });
  });

  it('does nothing with Right on a file with no definitions', () => {
    expect(keyMove(rows, 'f:src/main.physical.nsm', 'ArrowRight')).toBeNull();
  });

  it('closes an open folder with Left, else goes to the parent', () => {
    expect(keyMove(rows, 'd:module:proposed', 'ArrowLeft')).toEqual({ toggle: 'd:module:proposed', open: false });
    expect(keyMove(rows, 'd:module:parts', 'ArrowLeft')).toEqual({ focus: 's:module' });
    // A section that is closed, on its own at the top, has nowhere to go.
    expect(keyMove(flatten(tree, { 's:top_level': false }), 's:top_level', 'ArrowLeft')).toBeNull();
  });

  it('opens a file with Enter and toggles a folder', () => {
    expect(keyMove(rows, 'f:src/main.nsm', 'Enter')).toEqual({ activate: 'f:src/main.nsm' });
    expect(keyMove(rows, 'd:module:parts', 'Enter')).toEqual({ toggle: 'd:module:parts', open: true });
    expect(keyMove(rows, 'd:module:proposed', ' ')).toEqual({ toggle: 'd:module:proposed', open: false });
  });

  it('starts from the first row when nothing has the focus, and does nothing on an empty tree', () => {
    expect(keyMove(rows, null, 'ArrowDown')).toEqual({ focus: id(1) });
    expect(keyMove([], null, 'ArrowDown')).toBeNull();
  });
});

describe('file icons', () => {
  it('tells a file by its name', () => {
    expect(fileIcon('main.nsm')).toBe('nsm');
    expect(fileIcon('main.physical.nsm')).toBe('physical');
    expect(fileIcon('netsmith.toml')).toBe('toml');
    expect(fileIcon('Passives.CSV')).toBe('csv');
    expect(fileIcon('board.kicad_pcb')).toBe('kicad');
    expect(fileIcon('board.kicad_sch')).toBe('kicad');
    expect(fileIcon('README.md')).toBe('file');
  });
});
