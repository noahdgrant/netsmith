import { describe, expect, it } from 'vitest';

import { crumbOptions, crumbPath } from './crumbs';
import { scanSymbols } from './symbols';

const SOURCE = [
  'module LdoStage {',
  '    .vin: Rail',
  '    .input_cap: Capacitor',
  '}',
  '',
  'board PowerBoard {',
  '    .stage: LdoStage {',
  '        .vout.voltage &= 3.3V',
  '    }',
  '    .power_in: Header1x2',
  '}',
  '',
].join('\n');

describe('crumbPath', () => {
  const syms = scanSymbols(SOURCE);
  it('is the file, the definition and the declaration at the cursor', () => {
    const c = crumbPath('src/main.nsm', syms, 8);
    expect(c.map((x) => [x.kind, x.label])).toEqual([
      ['file', 'main.nsm'],
      ['def', 'PowerBoard'],
      ['decl', 'stage'],
    ]);
    expect(c[2].detail).toBe('LdoStage');
    expect(c[1].icon).toBe('board');
  });
  it('is the file and the definition on its header line', () => {
    expect(crumbPath('src/main.nsm', syms, 6).map((x) => x.label)).toEqual(['main.nsm', 'PowerBoard']);
  });
  it('is the file alone between definitions', () => {
    expect(crumbPath('src/main.nsm', syms, 5).map((x) => x.label)).toEqual(['main.nsm']);
  });
});

describe('crumbOptions', () => {
  const syms = scanSymbols(SOURCE);
  const files = ['src/main.nsm', 'src/power.nsm', 'src/sheets/adc.nsm', 'std/parts/passive.nsm'];
  it('lists the files in the same folder for the file crumb, the current one marked', () => {
    const c = crumbPath('src/main.nsm', syms, 8)[0];
    const o = crumbOptions(c, syms, files);
    expect(o.map((x) => [x.label, x.current])).toEqual([['main.nsm', true], ['power.nsm', false]]);
  });
  it('lists the definitions of the file for a definition crumb', () => {
    const c = crumbPath('src/main.nsm', syms, 8)[1];
    const o = crumbOptions(c, syms, files);
    expect(o.map((x) => [x.label, x.detail, x.line, x.current])).toEqual([
      ['LdoStage', 'module', 1, false],
      ['PowerBoard', 'board', 6, true],
    ]);
  });
  it('lists what is declared beside the declaration', () => {
    const c = crumbPath('src/main.nsm', syms, 8)[2];
    expect(crumbOptions(c, syms, files).map((x) => [x.label, x.detail, x.current])).toEqual([
      ['stage', 'LdoStage', true],
      ['power_in', 'Header1x2', false],
    ]);
  });
});
