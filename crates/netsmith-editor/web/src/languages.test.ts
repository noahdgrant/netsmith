import { describe, expect, it } from 'vitest';

import type { Problem } from './api';
import { explainInvalid, languageExtensions, languageOfPath, tomlLanguage } from './languages';
import { nsmLanguage } from './nsm';

function names(language: typeof tomlLanguage, text: string): string[] {
  const out: string[] = [];
  language.parser.parse(text).iterate({ enter: (n) => void out.push(n.name) });
  return out;
}

describe('languageOfPath', () => {
  it('gives the netsmith language to .nsm files only, a physical file included', () => {
    expect(languageOfPath('src/main.nsm')).toBe('nsm');
    expect(languageOfPath('src/main.physical.nsm')).toBe('nsm');
    expect(languageOfPath('SRC/Main.NSM')).toBe('nsm');
  });
  it('gives TOML to the manifest and the layout files', () => {
    expect(languageOfPath('netsmith.toml')).toBe('toml');
    expect(languageOfPath('src/main.pcb.toml')).toBe('toml');
  });
  it('gives plain text to a catalog and to anything else', () => {
    expect(languageOfPath('data/caps.csv')).toBe('plain');
    expect(languageOfPath('README.md')).toBe('plain');
    expect(languageOfPath('nsm')).toBe('plain');
    expect(languageOfPath('notes.nsm.txt')).toBe('plain');
  });
  it('adds the invalid-token colouring to .nsm files only', () => {
    const none = () => [];
    expect(languageExtensions('a.nsm', none).length).toBe(3);
    expect(languageExtensions('a.toml', none).length).toBe(2);
    expect(languageExtensions('a.csv', none)).toEqual([]);
    expect(languageExtensions('a.nsm', none)).toContain(nsmLanguage);
  });
});

describe('the TOML colouring', () => {
  const manifest = '# a project\n[project]\nname = "demo"\nlayers = [1, 2]\nfast = true\n\n[tool.kicad]\nversion = 9.0\nnote = """a\n  [b]\n"""\n';
  it('never marks anything as invalid', () => {
    expect(names(tomlLanguage, manifest)).not.toContain('invalid');
    expect(names(tomlLanguage, '[[bin]]\nx = { a = 1, b = \'c\' }\n= ] [ ?\n')).not.toContain('invalid');
  });
  it('colours tables, keys, strings, numbers and words', () => {
    const found = new Set(names(tomlLanguage, manifest));
    for (const n of ['heading', 'propertyName', 'string', 'number', 'bool', 'comment']) expect(found).toContain(n);
  });
});

describe('explainInvalid', () => {
  const problem = (line: number, column: number, endColumn: number): Problem =>
    ({ severity: 'error', message: 'expected a name\nmore', rendered: '', at: { file: 'a.nsm', line, column, endLine: line, endColumn } }) as Problem;
  it('says the compiler reports nothing when no problem is at the token', () => {
    const text = explainInvalid([], 'a.nsm', 3, 4, 5, ']');
    expect(text).toContain('unexpected `]`');
    expect(text).toContain('compiler reports no error here');
    expect(explainInvalid([problem(9, 1, 5)], 'a.nsm', 3, 4, 5, ']')).toContain('no error');
  });
  it('gives the checker\'s own message where it has a problem', () => {
    expect(explainInvalid([problem(3, 5, 6)], 'a.nsm', 3, 4, 5, ']')).toBe('expected a name');
  });
  it('does not take a problem of another file or a note', () => {
    expect(explainInvalid([{ ...problem(3, 5, 6), at: { file: 'b.nsm', line: 3, column: 5, endLine: 3, endColumn: 6 } }], 'a.nsm', 3, 4, 5, ']')).toContain('no error');
    expect(explainInvalid([{ ...problem(3, 5, 6), severity: 'note' }], 'a.nsm', 3, 4, 5, ']')).toContain('no error');
  });
});
