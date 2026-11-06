// A problem as a squiggle with a tooltip that is the page's card: what it is (severity and stage),
// the checker's message, where it is, and what to do (go to the part it is about, see all of them).

import { type Diagnostic } from '@codemirror/lint';
import { type EditorView, closeHoverTooltips } from '@codemirror/view';

import type { Location, Problem } from '../api';
import { ORIGIN_LABEL, describeProblem } from '../feedback/problems';
import { setDockOpen } from '../feedback/state';
import { moduleSel, partByPath, partSel, selectAndReveal } from '../selection';
import { chooseBoard, detail, revealSection } from '../store';
import { iconDom } from './dom';

const el = (tag: string, cls: string, text?: string) => {
  const n = document.createElement(tag);
  n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
};

/** What "Go to related" does for a problem: the part or module it is about, shown in the schematic;
 * else the board it is about; else nothing (a parse error has no related thing). */
export function relatedOf(p: Problem): { label: string; run: () => void } | null {
  const { path, board } = describeProblem(p);
  if (path) {
    const sel = partByPath(detail.peek(), path) ? partSel(path) : moduleSel(path);
    return {
      label: `Show ${path} in the schematic`,
      run: () => {
        selectAndReveal(sel);
        revealSection('schematic');
      },
    };
  }
  if (board) {
    return {
      label: `Show ${board}`,
      run: () => {
        chooseBoard(board);
        revealSection('schematic');
      },
    };
  }
  return null;
}

function place(at: Location | null): string {
  return at ? `${at.file}:${at.line}` : '';
}

/** The diagnostic for a problem, with a card for its tooltip. */
export function diagnosticOf(p: Problem, from: number, to: number): Diagnostic {
  const severity = p.severity === 'error' ? 'error' : 'warning';
  const info = describeProblem(p);
  const rel = relatedOf(p);
  return {
    from,
    to,
    severity,
    message: p.message,
    source: ORIGIN_LABEL[info.origin],
    renderMessage(view: EditorView) {
      const root = el('div', 'lt');
      root.setAttribute('data-testid', 'lint-card');
      const head = el('div', 'lt-head');
      head.append(iconDom(severity, 15, `lt-icon ${severity}`));
      head.append(el('span', 'lt-sev', severity === 'error' ? 'Error' : 'Warning'));
      const chip = el('span', 'lt-chip', ORIGIN_LABEL[info.origin]);
      chip.setAttribute('data-testid', 'lint-stage');
      head.append(chip);
      const lines = info.rest.split('\n');
      const msg = el('div', 'lt-msg');
      msg.append(el('div', 'lt-first', lines[0]));
      for (const l of lines.slice(1)) if (l.trim()) msg.append(el('div', 'lt-more', l));
      const meta = el('div', 'lt-meta');
      meta.append(el('span', 'lt-src', 'netsmith'));
      if (p.at) meta.append(el('span', 'lt-where', place(p.at)));
      if (info.path) meta.append(el('span', 'lt-path', info.path));
      const actions = el('div', 'lt-actions');
      const button = (label: string, run: () => void) => {
        const b = el('button', 'lt-btn', label) as HTMLButtonElement;
        b.type = 'button';
        b.addEventListener('click', () => {
          view.dispatch({ effects: closeHoverTooltips });
          run();
        });
        actions.append(b);
        return b;
      };
      if (rel) button('Go to related', rel.run).title = rel.label;
      button('All problems', () => setDockOpen(true));
      root.append(head, msg, meta, actions);
      return root;
    },
  };
}
