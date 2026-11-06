import { beforeEach, describe, expect, it } from 'vitest';

import { type DiagnosticsInput, NOT_REPORTED, buildDiagnostics, bundleOf, clearErrors, noteError, recentErrors } from './diagnostics';

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
  it('says what the server did not tell it, instead of leaving a line empty', () => {
    const text = buildDiagnostics(input());
    expect(text).toContain('netsmith version: not reported');
    expect(text).toContain('KiCad found: no');
    for (const w of NOT_REPORTED) expect(text).toContain(w);
  });
  it('copes with a project and an analysis that have not arrived', () => {
    const text = buildDiagnostics(input({ project: null, analysis: null, board: null, errors: [], firstProblems: [] }));
    expect(text).toContain('Project: not read yet');
    expect(text).toContain('No analysis yet');
    expect(text).toContain('Board shown: unknown');
    expect(text).toContain('none');
  });
  it('holds no folder of the project and no file contents', () => {
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
