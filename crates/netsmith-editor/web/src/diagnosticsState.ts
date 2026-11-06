// What the page knows right now, gathered into `diagnostics.ts`'s input, and the one action that copies it.

import { buildDiagnostics, bundleOf, recentErrors } from './diagnostics';
import { bundleCheck } from './bundleCheck';
import { allRows } from './feedback/derive';
import { busy, connection, notify } from './feedback/state';
import { CONNECTION_LABEL } from './feedback/status';
import { settings } from './settings';
import { analysis, board, layout, project, visible } from './store';
import { theme, themePref } from './theme';
import { copyText } from './components/ui';

export function gatherDiagnostics(): string {
  const a = analysis.peek();
  const p = project.peek();
  const vis = visible.peek();
  const focus = (vis.find((x) => x.index === layout.peek().focus) ?? vis[0])?.section ?? null;
  const rows = allRows.peek()
    .filter((r) => r.severity === 'error')
    .slice(0, 5)
    .map((r) => ({ severity: r.severity, message: r.message, where: r.at ? `${r.at.file}:${r.at.line}` : undefined }));
  const script = document.querySelector<HTMLScriptElement>('script[type="module"][src]');
  return buildDiagnostics({
    now: new Date(),
    bundle: bundleOf(script?.src),
    userAgent: navigator.userAgent,
    viewport: { width: window.innerWidth, height: window.innerHeight, ratio: window.devicePixelRatio || 1 },
    language: navigator.language,
    host: window.location.host,
    project: p ? { title: p.title, hasManifest: p.hasManifest, kicad: p.kicad, files: p.files.length, version: p.version, gitDescribe: p.gitDescribe, root: p.root, kicadPath: p.kicadPath, bundleHash: p.bundleHash } : null,
    bundleMatches: bundleCheck.peek(),
    analysis: a ? { version: a.version, errors: a.errors, warnings: a.warnings, notes: a.notes, elapsedMs: a.elapsedMs, boards: a.boards.map((b) => ({ name: b.name, stale: b.stale, parts: b.parts, nets: b.nets })) } : null,
    state: { connection: CONNECTION_LABEL[connection.peek()], analysis: busy.peek() ? 'Analysing' : a ? 'Analysed' : 'Not read yet' },
    board: board.peek(),
    section: focus,
    theme: `${theme.peek()}${themePref.peek() === 'system' ? ' (from the system)' : ''}`,
    settings: settings.peek(),
    firstProblems: rows,
    errors: recentErrors(),
  });
}

/** Copy the block and say so (or say it could not be copied). */
export async function copyDiagnostics(): Promise<boolean> {
  const ok = await copyText(gatherDiagnostics());
  notify(ok ? { title: 'Diagnostics copied', detail: 'Paste them into the issue: versions, the state of the analysis and the last errors.', kind: 'ok' } : { title: 'The diagnostics could not be copied', detail: 'This browser did not allow the clipboard.', kind: 'bad' });
  return ok;
}
