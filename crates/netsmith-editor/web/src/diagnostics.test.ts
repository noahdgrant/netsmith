import { beforeEach, describe, expect, it } from 'vitest';

import { type DiagnosticsInput, bundleInHtml, bundleMatch, buildDiagnostics, bundleOf, clearErrors, noteError, notReported, recentErrors } from './diagnostics';

const input = (over: Partial<DiagnosticsInput> = {}): DiagnosticsInput => ({
  now: new Date('2026-11-06T10:20:30Z'),
  bundle: 'index-4fa1b2.js',
  userAgent: 'Chrome/130',
  viewport: { width: 1280, height: 800, ratio: 1 },
  host: '127.0.0.1:8787',
  project: { title: 'Greenhouse', hasManifest: true, kicad: false, files: 9 },
  analysis: { version: 12, errors: 1, warnings: 2, notes: 0, elapsedMs: 412.6, boards: [{ name: 'Main/main', stale: true, parts: 5, nets: 3 }] },
  state: { connection: 'Live', analysis: 'Analysed' },
  board: 'Main/main',
  section: 'pcb',
  theme: 'dark',
  settings: { density: 'compact', scale: 110, motion: 'system' },
  firstProblems: [{ severity: 'error', message: 'unknown name `x`', where: 'src/main.nsm:4' }],
  errors: [{ at: Date.UTC(2026, 10, 6, 10, 19, 0), text: 'The router stopped' }],
  ...over,
});

describe('buildDiagnostics', () => {
  it('names the page, the server, the analysis, the settings and the last errors', () => {
    const text = buildDiagnostics(input());
    for (const w of ['index-4fa1b2.js', '127.0.0.1:8787', 'Greenhouse', 'density compact, size 110 %', 'Problems: 1 errors, 2 warnings, 0 notes', 'Main/main (stale) 5 parts 3 nets', 'error: unknown name `x` (src/main.nsm:4)', '10:19:00 The router stopped', 'Took: 413 ms']) {
      expect(text, w).toContain(w);
    }
  });
  it('says what an older server did not tell it, instead of leaving a line empty', () => {
    const text = buildDiagnostics(input());
    expect(text).toContain('netsmith version: not reported');
    expect(text).toContain('KiCad found: no');
    const missing = notReported(input().project);
    expect(missing).toEqual(['the version of netsmith', 'the commit it was built from', "the project's folder", 'the hash of the page the binary embeds']);
    for (const w of missing) expect(text).toContain(w);
  });
  it('reports the version, the commit, the folder, where KiCad is and the page hash when the server sends them', () => {
    const facts = { version: '0.1.0', gitDescribe: 'v0.1.0-12-g31f18d9', root: '/work/greenhouse', kicadPath: '/opt/kicad/bin/kicad-cli', bundleHash: '8bfd81b2f58bf920' };
    const project = { title: 'Greenhouse', hasManifest: true, kicad: true, files: 9, ...facts };
    const text = buildDiagnostics(input({ project, bundleMatches: 'match' }));
    for (const w of ['netsmith version: 0.1.0', 'Built from: v0.1.0-12-g31f18d9', 'Folder: /work/greenhouse', 'KiCad found: yes (/opt/kicad/bin/kicad-cli)', 'Embedded page (source hash): 8bfd81b2f58bf920', 'Page on screen vs the binary: match']) {
      expect(text, w).toContain(w);
    }
    expect(text).not.toContain('not reported');
    expect(text).not.toContain('Not reported');
    expect(notReported(project)).toEqual([]);
  });
  it('does not call a binary built outside a checkout, or without KiCad, an old server', () => {
    const project = { title: 'G', hasManifest: true, kicad: false, files: 1, version: '0.1.0', root: '/work/g' };
    expect(notReported(project)).toEqual([]);
    expect(notReported({ ...project, kicad: true, root: '/work/g' })).toEqual([]);
  });
  it('copes with a project and an analysis that have not arrived', () => {
    const text = buildDiagnostics(input({ project: null, analysis: null, board: null, errors: [], firstProblems: [] }));
    expect(text).toContain('Project: not read yet');
    expect(text).toContain('No analysis yet');
    expect(text).toContain('Board shown: unknown');
    expect(text).toContain('none');
  });
  it('holds no folder of the project when the server did not send it, and no file contents', () => {
    expect(buildDiagnostics(input())).not.toMatch(/\/home\/|[A-Z]:\\/);
  });
});

describe('the error log', () => {
  beforeEach(clearErrors);
  it('keeps the last eight, each on one line and not too long', () => {
    for (let i = 0; i < 12; i++) noteError(`failure ${i}\nwith a second line`, i);
    const log = recentErrors();
    expect(log).toHaveLength(8);
    expect(log[0].text).toBe('failure 4 with a second line');
    noteError('x'.repeat(1000));
    expect(recentErrors().at(-1)!.text).toHaveLength(300);
  });
  it('ignores an empty message', () => {
    noteError('   ');
    expect(recentErrors()).toHaveLength(0);
  });
});

describe('bundleOf', () => {
  it('is the file of the built script, and development for the dev server', () => {
    expect(bundleOf('http://127.0.0.1:8787/assets/index-4fa1b2.js')).toBe('index-4fa1b2.js');
    expect(bundleOf('http://127.0.0.1:5173/src/main.tsx')).toBe('development');
    expect(bundleOf(null)).toBe('development');
  });
});

describe('bundleMatch', () => {
  it('compares the name of the script on screen with the one the server serves', () => {
    expect(bundleMatch('index-4fa1b2.js', 'index-4fa1b2.js')).toBe('match');
    expect(bundleMatch('index-4fa1b2.js', 'index-9c0de1.js')).toBe('differs');
  });
  it('does not compare the dev server, or an answer it did not get', () => {
    expect(bundleMatch('development', 'index-4fa1b2.js')).toBe('unknown');
    expect(bundleMatch('index-4fa1b2.js', 'development')).toBe('unknown');
    expect(bundleMatch('index-4fa1b2.js', null)).toBe('unknown');
  });
});

describe('bundleInHtml', () => {
  it('reads the module script of the served page', () => {
    const html = '<html><head><script type="module" crossorigin src="/assets/index-CweXOqwh.js"></script></head></html>';
    expect(bundleInHtml(html)).toBe('index-CweXOqwh.js');
    expect(bundleInHtml('<html><script type="module" src="/src/main.tsx"></script></html>')).toBe('development');
    expect(bundleInHtml('<html></html>')).toBeNull();
  });
});
