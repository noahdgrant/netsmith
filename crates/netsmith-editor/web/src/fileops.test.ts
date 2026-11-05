import { describe, expect, it } from 'vitest';
import type { FileRole, TreeFile } from './api';
import {
  allowed,
  definitionName,
  folderFor,
  isPhysicalName,
  removalPrompt,
  starter,
  stemOf,
  suggestTemplate,
  validateName,
} from './fileops';
import { buildTree, filesOf, revealScroll, type FolderNode, type Node, type SectionNode } from './tree';

const file = (path: string, role: FileRole = 'module', ...kinds: string[]): TreeFile => ({
  path,
  role,
  readonly: role === 'std' || role === 'manifest',
  defs: kinds.map((kind, i) => ({ kind, name: `${kind}${i}`, line: 1 })),
});

describe('validateName', () => {
  it('adds the ending a source file has, and keeps a physical file', () => {
    expect(validateName('power', 'file')).toEqual({ ok: true, name: 'power.nsm' });
    expect(validateName('power.nsm', 'file')).toEqual({ ok: true, name: 'power.nsm' });
    expect(validateName('  power  ', 'file')).toEqual({ ok: true, name: 'power.nsm' });
    expect(validateName('power.physical', 'file')).toEqual({ ok: true, name: 'power.physical.nsm' });
    expect(validateName('power.physical.nsm', 'file')).toEqual({ ok: true, name: 'power.physical.nsm' });
    expect(validateName('Power.NSM', 'file')).toEqual({ ok: true, name: 'Power.NSM' });
  });

  it('refuses an empty name', () => {
    for (const raw of ['', '   ']) {
      const r = validateName(raw, 'file');
      expect(r.ok).toBe(false);
    }
  });

  it('refuses a slash, `..` and a name that is hidden', () => {
    for (const raw of ['a/b', 'a\\b', '../x', 'a..b', '..', '.', '.hidden', '/abs']) {
      for (const kind of ['file', 'folder'] as const) {
        expect(validateName(raw, kind).ok, `${raw} as a ${kind}`).toBe(false);
      }
    }
  });

  it('refuses what is not a module name', () => {
    for (const raw of ['a-b', 'a b', '1a', 'a.b.c', 'a$', 'power.txt', 'x:y']) {
      expect(validateName(raw, 'file').ok, raw).toBe(false);
    }
    for (const raw of ['a-b', 'a b', '1a', 'a.b']) {
      expect(validateName(raw, 'folder').ok, raw).toBe(false);
    }
    expect(validateName('sheets', 'folder')).toEqual({ ok: true, name: 'sheets' });
    expect(validateName('_x9', 'folder')).toEqual({ ok: true, name: '_x9' });
    expect(validateName('ström', 'folder').ok).toBe(true);
  });

  it('refuses the names some systems keep, and too long a name', () => {
    for (const raw of ['con', 'NUL', 'com1', 'LPT9']) {
      expect(validateName(raw, 'file').ok, raw).toBe(false);
      expect(validateName(raw, 'folder').ok, raw).toBe(false);
    }
    expect(validateName('a'.repeat(70), 'folder').ok).toBe(false);
  });

  it('says what is taken, in any case, and the message names it', () => {
    const r = validateName('power', 'file', ['Power.nsm', 'other.nsm']);
    expect(r.ok).toBe(false);
    expect(!r.ok && r.message).toContain('power.nsm');
    expect(validateName('power', 'folder', ['POWER']).ok).toBe(false);
    expect(validateName('power', 'folder', ['power.nsm']).ok).toBe(true);
  });
});

describe('names', () => {
  it('takes the stem and the physical file', () => {
    expect(stemOf('power.nsm')).toBe('power');
    expect(stemOf('power.physical.nsm')).toBe('power');
    expect(isPhysicalName('power.physical.nsm')).toBe(true);
    expect(isPhysicalName('power.nsm')).toBe(false);
  });
  it('makes a definition name from a stem', () => {
    expect(definitionName('power_stage')).toBe('PowerStage');
    expect(definitionName('adc')).toBe('Adc');
    expect(definitionName('_x')).toBe('X');
    expect(definitionName('already_Mixed')).toBe('AlreadyMixed');
  });
  it('guesses what a file holds from its name', () => {
    expect(suggestTemplate('sensor_board')).toBe('board');
    expect(suggestTemplate('board')).toBe('board');
    expect(suggestTemplate('main_board_v2')).toBe('board');
    expect(suggestTemplate('imu_part')).toBe('part');
    expect(suggestTemplate('power_stage')).toBe('module');
    expect(suggestTemplate('keyboard')).toBe('board');
  });
});

describe('starter', () => {
  it('is a comment for an empty file, with the name of the definition in the others', () => {
    expect(starter('empty', 'power_stage')).toBe('//! PowerStage.\n');
    expect(starter('module', 'power_stage')).toContain('module PowerStage {');
    expect(starter('board', 'main')).toContain('board Main {');
    expect(starter('part', 'imu')).toContain('part Imu {');
  });
  it('imports what it uses', () => {
    expect(starter('module', 'x')).toContain('import std::sources::Rail');
    expect(starter('board', 'x')).toContain('import std::mfg::jlcpcb::TwoLayer');
    expect(starter('board', 'x')).toContain('.stackup.thickness &= 1.6mm');
  });
  it('is a physical unit for the board of the source beside it', () => {
    expect(starter('physical', 'main', 'Greenhouse')).toContain('physical for Greenhouse {\n    case {\n    }\n}');
    expect(starter('physical', 'main')).toContain('physical for Main {');
  });
  it('is made of lines with no tabs or trailing spaces, and ends in a newline', () => {
    for (const t of ['empty', 'module', 'board', 'part', 'physical'] as const) {
      const text = starter(t, 'sample_name', 'Sample');
      expect(text.endsWith('\n') && !text.endsWith('\n\n'), t).toBe(true);
      expect(text.includes('\t'), t).toBe(false);
      expect(text.split('\n').every((l) => l === l.trimEnd()), t).toBe(true);
    }
  });
});

describe('where and what', () => {
  const files = [
    file('src/main.nsm', 'top_level', 'board'),
    file('src/sheets/adc.nsm', 'module', 'module'),
    file('src/sheets/adc.physical.nsm', 'physical'),
    file('elsewhere/top.nsm', 'top_level', 'board'),
    file('netsmith.toml', 'manifest'),
    file('std/bus.nsm', 'std', 'interface'),
  ];
  const tree = buildTree(files, ['src/empty', 'src/sheets']);
  const nodes = (): Node[] => {
    const out: Node[] = [];
    const walk = (n: Node) => {
      out.push(n);
      if (n.type !== 'def') n.children.forEach(walk);
    };
    tree.forEach(walk);
    return out;
  };
  const folder = (dir: string) => nodes().find((n) => n.type === 'folder' && n.dir === dir) as FolderNode;
  const fileNode = (path: string) => filesOf(tree).find((f) => f.file.path === path)!;
  const section = (role: string) => tree.find((s) => s.role === role) as SectionNode;

  it('gives a folder its path from the project root', () => {
    expect(folder('src/sheets').dir).toBe('src/sheets');
    expect(folder('std/mfg')?.dir ?? 'none').toBe('none');
    expect(folder('elsewhere').dir).toBe('elsewhere');
  });

  it('shows a folder that has no file in it, once', () => {
    const empty = folder('src/empty');
    expect(empty.files).toBe(0);
    expect(empty.children).toEqual([]);
    expect(nodes().filter((n) => n.type === 'folder' && n.dir === 'src/sheets').length).toBe(2); // modules and physical
    expect(section('module').children.map((c) => (c.type === 'folder' ? c.dir : c.file.path))).toEqual(['src/empty', 'src/sheets']);
  });

  it('means a folder by "new here", or the folder a file is in, else src', () => {
    expect(folderFor(folder('src/sheets'))).toBe('src/sheets');
    expect(folderFor(fileNode('src/sheets/adc.nsm'))).toBe('src/sheets');
    expect(folderFor(fileNode('src/main.nsm'))).toBe('src');
    expect(folderFor(fileNode('elsewhere/top.nsm'))).toBe('src');
    expect(folderFor(folder('elsewhere'))).toBe('src');
    expect(folderFor(fileNode('std/bus.nsm'))).toBe('src');
    expect(folderFor(section('module'))).toBe('src');
    expect(folderFor(null)).toBe('src');
  });

  it('says what a row may do, and why not', () => {
    expect(allowed(fileNode('src/sheets/adc.nsm'))).toEqual({ newHere: true, rename: null, remove: null });
    expect(allowed(folder('src/sheets'))).toEqual({ newHere: true, rename: null, remove: null });
    expect(allowed(folder('elsewhere')).rename).toContain('below src/');
    expect(allowed(fileNode('src/main.nsm')).rename).toContain('netsmith.toml');
    expect(allowed(fileNode('src/main.nsm')).remove).toBeNull();
    expect(allowed(fileNode('src/sheets/adc.physical.nsm')).rename).toContain('source');
    expect(allowed(fileNode('netsmith.toml')).remove).toContain('outside the editor');
    expect(allowed(fileNode('std/bus.nsm')).rename).toContain('standard library');
    expect(allowed(section('module')).rename).toContain('Select');
  });

  it('asks for the stronger yes for a top-level file and a folder with files', () => {
    expect(removalPrompt(fileNode('src/main.nsm'), []).strong).toBe(true);
    expect(removalPrompt(fileNode('src/sheets/adc.nsm'), []).strong).toBe(false);
    expect(removalPrompt(folder('src/sheets'), []).strong).toBe(true);
    expect(removalPrompt(folder('src/empty'), []).strong).toBe(false);
    const p = removalPrompt(fileNode('src/sheets/adc.nsm'), ['src/sheets/adc.nsm']);
    expect(p.title).toBe('Delete src/sheets/adc.nsm?');
    expect(p.lines.join(' ')).toContain('Unsaved changes in src/sheets/adc.nsm are lost.');
    expect(p.lines.join(' ')).toContain('cannot be undone');
  });
});

describe('revealScroll', () => {
  it('leaves the list alone when the row is all in view', () => {
    expect(revealScroll(400, 26, 300, 400)).toBe(300);
    expect(revealScroll(300, 26, 300, 400)).toBe(300);
    expect(revealScroll(674, 26, 300, 400)).toBe(300);
  });
  it('moves it as little as can be when the row is above or below', () => {
    expect(revealScroll(100, 26, 300, 400)).toBe(100);
    expect(revealScroll(700, 26, 300, 400)).toBe(326);
  });
  it('does not leave a row under the sticky header', () => {
    expect(revealScroll(310, 26, 300, 400, 32)).toBe(278);
    expect(revealScroll(340, 26, 300, 400, 32)).toBe(300);
    expect(revealScroll(10, 26, 300, 400, 32)).toBe(0);
  });
});
