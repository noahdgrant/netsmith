// What "Report a problem" copies: a block of plain text a person can paste into an issue. It says what the page is (the bundle,
// the browser, the window), what the server told it (the project, whether KiCad was found, the state of the analysis), how the
// page is set, and the last errors it showed. Nothing in it leaves the browser by itself, and it holds no file contents and no
// path of the project: the person sees it before it goes anywhere.
//
// What the page cannot say today, because the server does not report it: the version of `netsmith`, the project's folder and
// where KiCad was found (`/api/project` has only `title`, `kicad: boolean`, the files). `NOT_REPORTED` lists them and the block
// says so, so a reader knows the line is missing and not empty.

/** What `/api/project` should carry so that the About dialog and this block can be complete. */
export const NOT_REPORTED = ['the version of netsmith', "the project's folder", 'where KiCad was found'] as const;

const LAST = 8;
const errors: { at: number; text: string }[] = [];

/** Remember an error the page showed or hit (the last few are kept). */
export function noteError(text: string, at = Date.now()) {
  const one = text.replace(/\s+/g, ' ').trim().slice(0, 300);
  if (!one) return;
  errors.push({ at, text: one });
  if (errors.length > LAST) errors.shift();
}

export function recentErrors(): { at: number; text: string }[] {
  return [...errors];
}

export function clearErrors() {
  errors.length = 0;
}

export interface DiagnosticsInput {
  now: Date;
  /** The name of the bundle's main script (`index-4fa1b2.js`), or `development`. */
  bundle: string;
  userAgent: string;
  viewport: { width: number; height: number; ratio: number };
  language?: string;
  /** The page's address without the path: where the editor listens. */
  host: string;
  project: { title: string; hasManifest: boolean; kicad: boolean; files: number } | null;
  analysis: {
    version: number;
    errors: number;
    warnings: number;
    notes: number;
    elapsedMs?: number;
    boards: { name: string; stale: boolean; parts: number; nets: number }[];
  } | null;
  /** The status bar's words for the socket and the analysis. */
  state: { connection: string; analysis: string };
  board: string | null;
  section: string | null;
  theme: string;
  settings: { density: string; scale: number; motion: string };
  firstProblems: { severity: string; message: string; where?: string }[];
  errors: { at: number; text: string }[];
}

const line = (label: string, value: string | number | boolean | null | undefined) => `${label}: ${value === null || value === undefined || value === '' ? 'unknown' : value}`;

/** The block. Stable in its order, so two reports can be compared by eye. */
export function buildDiagnostics(d: DiagnosticsInput): string {
  const out: string[] = [];
  out.push('netsmith editor: diagnostics');
  out.push(line('When', d.now.toISOString()));
  out.push('');
  out.push('Page');
  out.push(line('  Bundle', d.bundle));
  out.push(line('  Served by', d.host));
  out.push(line('  Browser', d.userAgent));
  out.push(line('  Window', `${d.viewport.width} x ${d.viewport.height} px at ${d.viewport.ratio}x${d.language ? `, ${d.language}` : ''}`));
  out.push(line('  Theme', d.theme));
  out.push(line('  Settings', `density ${d.settings.density}, size ${d.settings.scale} %, motion ${d.settings.motion}`));
  out.push(line('  Section', d.section));
  out.push('');
  out.push('Server');
  out.push(`  netsmith version: not reported (the server does not send it)`);
  out.push(line('  Connection', d.state.connection));
  if (d.project) {
    out.push(line('  Project', d.project.title));
    out.push(line('  Manifest (netsmith.toml)', d.project.hasManifest ? 'yes' : 'no'));
    out.push(line('  Files', d.project.files));
    out.push(`  KiCad found: ${d.project.kicad ? 'yes (where: not reported)' : 'no'}`);
  } else {
    out.push('  Project: not read yet');
  }
  out.push('');
  out.push('Analysis');
  out.push(line('  State', d.state.analysis));
  if (d.analysis) {
    out.push(line('  Version', d.analysis.version));
    out.push(`  Problems: ${d.analysis.errors} errors, ${d.analysis.warnings} warnings, ${d.analysis.notes} notes`);
    out.push(line('  Took', d.analysis.elapsedMs === undefined ? undefined : `${Math.round(d.analysis.elapsedMs)} ms`));
    out.push(`  Boards (${d.analysis.boards.length}): ${d.analysis.boards.map((b) => `${b.name}${b.stale ? ' (stale)' : ''} ${b.parts} parts ${b.nets} nets`).join('; ') || 'none'}`);
  } else {
    out.push('  No analysis yet');
  }
  out.push(line('  Board shown', d.board));
  if (d.firstProblems.length) {
    out.push('  First problems:');
    for (const p of d.firstProblems) out.push(`    ${p.severity}: ${p.message}${p.where ? ` (${p.where})` : ''}`);
  }
  out.push('');
  out.push('Last errors the page showed');
  if (d.errors.length === 0) out.push('  none');
  for (const e of d.errors) out.push(`  ${new Date(e.at).toISOString().slice(11, 19)} ${e.text}`);
  out.push('');
  out.push(`Not reported by the server, so not in this block: ${NOT_REPORTED.join('; ')}.`);
  return out.join('\n');
}

/** The bundle's name from the page's own module script: `assets/index-4fa1b2.js` is `index-4fa1b2.js`; the dev server's is `development`. */
export function bundleOf(src: string | null | undefined): string {
  if (!src) return 'development';
  const m = /\/assets\/([^/?#]+)/.exec(src);
  return m ? m[1] : 'development';
}
